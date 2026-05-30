#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <Contacts/Contacts.h>
#import <CoreBluetooth/CoreBluetooth.h>
#import <CoreLocation/CoreLocation.h>
#import <QuartzCore/QuartzCore.h>
#import <CoreVideo/CoreVideo.h>
#import <Metal/Metal.h>
#import <Network/Network.h>
#import <Photos/Photos.h>
#import <Security/Security.h>
#import <UserNotifications/UserNotifications.h>
#import <WebKit/WebKit.h>
#import <dispatch/dispatch.h>
#import <math.h>
#import <string.h>

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

@interface AppDelegate : NSObject <NSApplicationDelegate, UNUserNotificationCenterDelegate>
@property (strong) NSWindow *window;
@property (nonatomic) dispatch_source_t memorySource;
@end

static void MacEmitPermission(uint32_t domain, uint32_t status);
uint32_t macos_permission_status(uint32_t domain);

@interface OxideMacLocationPermissionDelegate : NSObject <CLLocationManagerDelegate>
@end

@implementation OxideMacLocationPermissionDelegate
- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager
{
    (void)manager;
    MacEmitPermission(1u, macos_permission_status(1u));
}

- (void)locationManager:(CLLocationManager *)manager didChangeAuthorizationStatus:(CLAuthorizationStatus)status
{
    (void)manager;
    (void)status;
    MacEmitPermission(1u, macos_permission_status(1u));
}
@end

typedef struct {
    double latitude;
    double longitude;
    double altitude;
    double horizontal_accuracy;
    double vertical_accuracy;
    double speed;
    double course;
    uint64_t timestamp_ms;
} OxideLocationSample;

typedef struct {
    uint32_t accuracy_kind;
    double distance_filter_m;
    uint8_t allow_background;
    uint8_t precise;
} OxideLocationConfig;

typedef struct {
    double pressure_pa;
    double relative_altitude_m;
    uint64_t timestamp_ms;
    uint8_t has_pressure;
    uint8_t has_relative_altitude;
} OxideMotionSample;

typedef struct {
    const uint8_t *y_ptr;
    size_t y_len;
    size_t y_stride;
    const uint8_t *uv_ptr;
    size_t uv_len;
    size_t uv_stride;
    int32_t width;
    int32_t height;
    uint64_t timestamp_ns;
    uint16_t rotation_deg;
    uint8_t bit_depth;
    uint8_t matrix;
    uint8_t video_range;
} OxideCamFrame;

typedef struct {
    const int16_t *audio_ptr;
    size_t sample_count;
    uint32_t channels;
    uint32_t sample_rate_hz;
    uint64_t timestamp_ns;
} OxideCamAudio;

typedef struct {
    uint32_t kind;
    const uint8_t *path_ptr;
    size_t path_len;
    uint64_t duration_ns;
    uint64_t size_bytes;
    uint8_t had_audio;
    int32_t error_code;
    const uint8_t *error_msg_ptr;
    size_t error_msg_len;
} OxideCamRecordEvent;

typedef struct {
    uint32_t kind;
    OxideCamFrame frame;
    int32_t error_code;
    const uint8_t *error_msg_ptr;
    size_t error_msg_len;
} OxideCamPhotoEvent;

typedef struct {
    int32_t width;
    int32_t height;
    float fps_min;
    float fps_max;
    uint32_t color_spaces_mask;
} OxideCamFormat;

typedef struct {
    uint32_t code;
    int32_t bit_depth;
    int32_t range;
} OxideCamPixFmt;

typedef void (*OxideCameraFrameCallback)(const OxideCamFrame *);
typedef void (*OxideCameraAudioCallback)(const OxideCamAudio *);
typedef void (*OxideCameraRecordCallback)(const OxideCamRecordEvent *);
typedef void (*OxideCameraPhotoCallback)(const OxideCamPhotoEvent *);

static OxideCameraFrameCallback gOxideCameraFrameCallback = NULL;
static OxideCameraAudioCallback gOxideCameraAudioCallback = NULL;
static OxideCameraRecordCallback gOxideCameraRecordCallback = NULL;
static OxideCameraPhotoCallback gOxideCameraPhotoCallback = NULL;
static uint8_t gOxideCameraRunning = 0;
static char gOxideCameraQueueKey;

enum {
    OxideCamErrUnsupported = -1,
    OxideCamErrPermissionDenied = -2,
    OxideCamErrNotFound = -3,
    OxideCamErrBusy = -4,
    OxideCamErrInvalid = -5,
    OxideCamErrIo = -6,
};

static uint64_t MacSampleTimestampNs(CMSampleBufferRef sample)
{
    CMTime pts = CMSampleBufferGetPresentationTimeStamp(sample);
    if (!CMTIME_IS_VALID(pts) || pts.timescale == 0) {
        return (uint64_t)(CACurrentMediaTime() * 1000000000.0);
    }
    CMTime ns = CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default);
    if (!CMTIME_IS_VALID(ns) || ns.timescale == 0 || ns.value < 0) {
        return (uint64_t)(CACurrentMediaTime() * 1000000000.0);
    }
    return (uint64_t)ns.value;
}

static dispatch_queue_t MacCameraQueue(void)
{
    static dispatch_queue_t queue;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        queue = dispatch_queue_create("com.oxide.macos.camera", DISPATCH_QUEUE_SERIAL);
        dispatch_queue_set_specific(queue, &gOxideCameraQueueKey, &gOxideCameraQueueKey, NULL);
    });
    return queue;
}

static void MacCameraSync(dispatch_block_t block)
{
    if (dispatch_get_specific(&gOxideCameraQueueKey) != NULL) {
        block();
    } else {
        dispatch_sync(MacCameraQueue(), block);
    }
}

static AVCaptureSessionPreset MacCameraPresetForHeight(uint32_t height)
{
    if (height >= 2160) return AVCaptureSessionPreset3840x2160;
    if (height >= 1080) return AVCaptureSessionPreset1920x1080;
    if (height >= 720) return AVCaptureSessionPreset1280x720;
    return AVCaptureSessionPreset640x480;
}

static NSArray<AVCaptureDeviceType> *MacCameraDeviceTypes(void)
{
    NSMutableArray<AVCaptureDeviceType> *types = [[NSMutableArray alloc] init];
    [types addObject:AVCaptureDeviceTypeBuiltInWideAngleCamera];
    if (@available(macOS 14.0, *)) {
        [types addObject:AVCaptureDeviceTypeExternal];
        [types addObject:AVCaptureDeviceTypeContinuityCamera];
    }
    return types;
}

static AVCaptureDevice *MacCameraDeviceForPosition(NSInteger position)
{
    AVCaptureDevicePosition requested =
        position == 1 ? AVCaptureDevicePositionFront : AVCaptureDevicePositionUnspecified;
    AVCaptureDeviceDiscoverySession *session =
        [AVCaptureDeviceDiscoverySession discoverySessionWithDeviceTypes:MacCameraDeviceTypes()
                                                               mediaType:AVMediaTypeVideo
                                                                position:requested];
    AVCaptureDevice *device = session.devices.firstObject;
    if (device != nil) return device;
    AVCaptureDeviceDiscoverySession *fallback =
        [AVCaptureDeviceDiscoverySession discoverySessionWithDeviceTypes:MacCameraDeviceTypes()
                                                               mediaType:AVMediaTypeVideo
                                                                position:AVCaptureDevicePositionUnspecified];
    return fallback.devices.firstObject;
}

static AVCaptureDevice *MacAudioDevice(void)
{
    if (@available(macOS 14.0, *)) {
        AVCaptureDeviceDiscoverySession *audioDevices =
            [AVCaptureDeviceDiscoverySession discoverySessionWithDeviceTypes:@[ AVCaptureDeviceTypeMicrophone ]
                                                                   mediaType:AVMediaTypeAudio
                                                                    position:AVCaptureDevicePositionUnspecified];
        AVCaptureDevice *device = audioDevices.devices.firstObject;
        if (device != nil) return device;
    }
    return [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
}

@interface OxideMacCameraController : NSObject <AVCaptureVideoDataOutputSampleBufferDelegate, AVCaptureAudioDataOutputSampleBufferDelegate, AVCaptureFileOutputRecordingDelegate>
@property (nonatomic, strong) AVCaptureSession *session;
@property (nonatomic, strong) AVCaptureDeviceInput *videoInput;
@property (nonatomic, strong) AVCaptureDeviceInput *audioInput;
@property (nonatomic, strong) AVCaptureVideoDataOutput *videoOutput;
@property (nonatomic, strong) AVCaptureAudioDataOutput *audioOutput;
@property (nonatomic, strong) AVCaptureMovieFileOutput *movieOutput;
@property (nonatomic) uint32_t width;
@property (nonatomic) uint32_t height;
@property (nonatomic) uint32_t fps;
@property (nonatomic) NSInteger position;
@property (nonatomic) NSInteger mode;
@property (nonatomic) int32_t bitDepth;
@property (nonatomic) int32_t colorSpace;
@property (nonatomic) BOOL deliverFrames;
@property (nonatomic) BOOL enableAudio;
@property (nonatomic) BOOL pendingPhoto;
@property (nonatomic) BOOL recordCancelled;
@property (nonatomic) BOOL recordHadAudio;
@property (nonatomic, strong) NSURL *recordingURL;
@end

@implementation OxideMacCameraController

- (instancetype)init
{
    self = [super init];
    if (self) {
        _width = 1920;
        _height = 1080;
        _fps = 30;
        _position = 0;
        _mode = 0;
        _bitDepth = 8;
        _colorSpace = 0;
    }
    return self;
}

- (BOOL)videoAuthorized
{
    return [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo] ==
        AVAuthorizationStatusAuthorized;
}

- (BOOL)audioAuthorized
{
    return [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio] ==
        AVAuthorizationStatusAuthorized;
}

- (int32_t)configureSessionDeliverFrames:(BOOL)deliverFrames enableAudio:(BOOL)enableAudio
{
    if (![self videoAuthorized]) return OxideCamErrPermissionDenied;
    AVCaptureDevice *device = MacCameraDeviceForPosition(self.position);
    if (device == nil) return OxideCamErrNotFound;

    NSError *error = nil;
    AVCaptureDeviceInput *videoInput =
        [AVCaptureDeviceInput deviceInputWithDevice:device error:&error];
    if (videoInput == nil || error != nil) return OxideCamErrUnsupported;

    AVCaptureSession *session = [[AVCaptureSession alloc] init];
    AVCaptureSessionPreset preset = MacCameraPresetForHeight(self.height);
    if ([session canSetSessionPreset:preset]) session.sessionPreset = preset;

    [session beginConfiguration];
    if ([session canAddInput:videoInput]) {
        [session addInput:videoInput];
    } else {
        [session commitConfiguration];
        return OxideCamErrUnsupported;
    }

    AVCaptureVideoDataOutput *videoOutput = [[AVCaptureVideoDataOutput alloc] init];
    videoOutput.alwaysDiscardsLateVideoFrames = YES;
    videoOutput.videoSettings = @{
        (NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
    };
    [videoOutput setSampleBufferDelegate:self queue:MacCameraQueue()];
    if ([session canAddOutput:videoOutput]) {
        [session addOutput:videoOutput];
    } else {
        [session commitConfiguration];
        return OxideCamErrUnsupported;
    }

    AVCaptureDeviceInput *audioInput = nil;
    AVCaptureAudioDataOutput *audioOutput = nil;
    if (enableAudio && [self audioAuthorized]) {
        AVCaptureDevice *audioDevice = MacAudioDevice();
        if (audioDevice != nil) {
            audioInput = [AVCaptureDeviceInput deviceInputWithDevice:audioDevice error:&error];
            if (audioInput != nil && [session canAddInput:audioInput]) {
                [session addInput:audioInput];
                audioOutput = [[AVCaptureAudioDataOutput alloc] init];
                [audioOutput setSampleBufferDelegate:self queue:MacCameraQueue()];
                if ([session canAddOutput:audioOutput]) {
                    [session addOutput:audioOutput];
                }
            }
        }
    }

    [session commitConfiguration];
    self.session = session;
    self.videoInput = videoInput;
    self.audioInput = audioInput;
    self.videoOutput = videoOutput;
    self.audioOutput = audioOutput;
    self.deliverFrames = deliverFrames;
    self.enableAudio = enableAudio && audioInput != nil;
    return 0;
}

- (int32_t)startDeliverFrames:(BOOL)deliverFrames enableAudio:(BOOL)enableAudio
{
    [self stop];
    int32_t configureResult = [self configureSessionDeliverFrames:deliverFrames
                                                      enableAudio:enableAudio];
    if (configureResult != 0) return configureResult;
    [self.session startRunning];
    gOxideCameraRunning = self.session.isRunning ? 1 : 0;
    return self.session.isRunning ? 0 : OxideCamErrUnsupported;
}

- (void)stop
{
    if (self.movieOutput.isRecording) [self.movieOutput stopRecording];
    [self.session stopRunning];
    self.session = nil;
    self.videoInput = nil;
    self.audioInput = nil;
    self.videoOutput = nil;
    self.audioOutput = nil;
    self.movieOutput = nil;
    self.pendingPhoto = NO;
    gOxideCameraRunning = 0;
}

- (int32_t)ensureMovieOutputWithAudio:(BOOL)includeAudio
{
    if (self.session == nil || !self.session.isRunning) return OxideCamErrNotFound;
    if (includeAudio && self.audioInput == nil && [self audioAuthorized]) {
        NSError *error = nil;
        AVCaptureDevice *audioDevice = MacAudioDevice();
        AVCaptureDeviceInput *input = audioDevice == nil ? nil :
            [AVCaptureDeviceInput deviceInputWithDevice:audioDevice error:&error];
        if (input != nil && error == nil) {
            [self.session beginConfiguration];
            if ([self.session canAddInput:input]) {
                [self.session addInput:input];
                self.audioInput = input;
            }
            [self.session commitConfiguration];
        }
    }
    if (self.movieOutput == nil) {
        AVCaptureMovieFileOutput *output = [[AVCaptureMovieFileOutput alloc] init];
        [self.session beginConfiguration];
        if ([self.session canAddOutput:output]) {
            [self.session addOutput:output];
            self.movieOutput = output;
        }
        [self.session commitConfiguration];
    }
    return self.movieOutput != nil ? 0 : OxideCamErrUnsupported;
}

- (int32_t)startRecordingPath:(const uint8_t *)pathPtr
                          len:(size_t)pathLen
                    container:(int32_t)container
                 includeAudio:(BOOL)includeAudio
{
    if (container < 0 || container > 1) return OxideCamErrInvalid;
    int32_t outputResult = [self ensureMovieOutputWithAudio:includeAudio];
    if (outputResult != 0) return outputResult;
    if (self.movieOutput.isRecording) return OxideCamErrBusy;

    NSString *path = nil;
    if (pathPtr != NULL && pathLen > 0) {
        path = [[NSString alloc] initWithBytes:pathPtr length:pathLen encoding:NSUTF8StringEncoding];
        if (path == nil) return OxideCamErrInvalid;
    }
    if (path.length == 0) {
        NSString *ext = container == 1 ? @"mov" : @"mp4";
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:
            [NSString stringWithFormat:@"oxide-camera-%@.%@", NSUUID.UUID.UUIDString, ext]];
    }
    NSURL *url = [NSURL fileURLWithPath:path];
    if (url == nil) return OxideCamErrInvalid;
    [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
    self.recordCancelled = NO;
    self.recordHadAudio = includeAudio && self.audioInput != nil;
    self.recordingURL = url;
    [self.movieOutput startRecordingToOutputFileURL:url recordingDelegate:self];
    return 0;
}

- (int32_t)capturePhotoHighSpeed:(BOOL)highSpeed flashMode:(int32_t)flashMode
{
    (void)highSpeed;
    if (flashMode != 0) return OxideCamErrUnsupported;
    if (self.session == nil || !self.session.isRunning) return OxideCamErrNotFound;
    if (self.pendingPhoto) return OxideCamErrBusy;
    self.pendingPhoto = YES;
    return 0;
}

- (int32_t)setFocusX:(float)x y:(float)y
{
    AVCaptureDevice *device = self.videoInput.device;
    if (device == nil) return OxideCamErrNotFound;
    if (!device.isFocusPointOfInterestSupported) return OxideCamErrUnsupported;
    NSError *error = nil;
    if (![device lockForConfiguration:&error] || error != nil) return OxideCamErrBusy;
    CGPoint point = CGPointMake(fmax(0.0, fmin(1.0, x)), fmax(0.0, fmin(1.0, y)));
    BOOL applied = NO;
    if ([device isFocusModeSupported:AVCaptureFocusModeContinuousAutoFocus]) {
        device.focusPointOfInterest = point;
        device.focusMode = AVCaptureFocusModeContinuousAutoFocus;
        applied = YES;
    }
    if (device.isExposurePointOfInterestSupported &&
        [device isExposureModeSupported:AVCaptureExposureModeContinuousAutoExposure]) {
        device.exposurePointOfInterest = point;
        device.exposureMode = AVCaptureExposureModeContinuousAutoExposure;
        applied = YES;
    }
    [device unlockForConfiguration];
    return applied ? 0 : OxideCamErrUnsupported;
}

- (void)captureOutput:(AVCaptureOutput *)output
 didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
       fromConnection:(AVCaptureConnection *)connection
{
    (void)connection;
    if (output == self.videoOutput) {
        CVImageBufferRef image = CMSampleBufferGetImageBuffer(sampleBuffer);
        if (image == NULL || CVPixelBufferGetPlaneCount(image) < 2) return;
        CVPixelBufferLockBaseAddress(image, kCVPixelBufferLock_ReadOnly);
        uint8_t *y = CVPixelBufferGetBaseAddressOfPlane(image, 0);
        uint8_t *uv = CVPixelBufferGetBaseAddressOfPlane(image, 1);
        size_t width = CVPixelBufferGetWidth(image);
        size_t height = CVPixelBufferGetHeight(image);
        size_t yStride = CVPixelBufferGetBytesPerRowOfPlane(image, 0);
        size_t uvStride = CVPixelBufferGetBytesPerRowOfPlane(image, 1);
        size_t uvHeight = CVPixelBufferGetHeightOfPlane(image, 1);
        if (y != NULL && uv != NULL && width > 0 && height > 0) {
            OxideCamFrame frame = {
                .y_ptr = y,
                .y_len = yStride * height,
                .y_stride = yStride,
                .uv_ptr = uv,
                .uv_len = uvStride * uvHeight,
                .uv_stride = uvStride,
                .width = (int32_t)width,
                .height = (int32_t)height,
                .timestamp_ns = MacSampleTimestampNs(sampleBuffer),
                .rotation_deg = 0,
                .bit_depth = 8,
                .matrix = 1,
                .video_range = 1,
            };
            if (self.deliverFrames && gOxideCameraFrameCallback != NULL) {
                gOxideCameraFrameCallback(&frame);
            }
            if (self.pendingPhoto && gOxideCameraPhotoCallback != NULL) {
                self.pendingPhoto = NO;
                OxideCamPhotoEvent event = {
                    .kind = 0,
                    .frame = frame,
                    .error_code = 0,
                    .error_msg_ptr = NULL,
                    .error_msg_len = 0,
                };
                gOxideCameraPhotoCallback(&event);
            }
        }
        CVPixelBufferUnlockBaseAddress(image, kCVPixelBufferLock_ReadOnly);
        return;
    }

    if (output == self.audioOutput && gOxideCameraAudioCallback != NULL) {
        CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sampleBuffer);
        if (block == NULL) return;
        size_t len = CMBlockBufferGetDataLength(block);
        if (len < sizeof(int16_t)) return;
        char *data = NULL;
        OSStatus status = CMBlockBufferGetDataPointer(block, 0, NULL, NULL, &data);
        NSMutableData *copy = nil;
        if (status != noErr || data == NULL) {
            copy = [[NSMutableData alloc] initWithLength:len];
            if (CMBlockBufferCopyDataBytes(block, 0, len, copy.mutableBytes) != noErr) return;
            data = copy.mutableBytes;
        }
        const AudioStreamBasicDescription *desc =
            CMAudioFormatDescriptionGetStreamBasicDescription(CMSampleBufferGetFormatDescription(sampleBuffer));
        OxideCamAudio audio = {
            .audio_ptr = (const int16_t *)data,
            .sample_count = len / sizeof(int16_t),
            .channels = desc == NULL ? 1 : desc->mChannelsPerFrame,
            .sample_rate_hz = desc == NULL ? 0 : (uint32_t)desc->mSampleRate,
            .timestamp_ns = MacSampleTimestampNs(sampleBuffer),
        };
        gOxideCameraAudioCallback(&audio);
    }
}

- (void)captureOutput:(AVCaptureFileOutput *)output
didFinishRecordingToOutputFileAtURL:(NSURL *)outputFileURL
      fromConnections:(NSArray<AVCaptureConnection *> *)connections
                error:(NSError *)error
{
    (void)output;
    (void)connections;
    if (gOxideCameraRecordCallback == NULL) return;
    NSString *path = outputFileURL.path ?: @"";
    NSData *pathData = [path dataUsingEncoding:NSUTF8StringEncoding] ?: [NSData data];
    uint64_t size = 0;
    NSNumber *fileSize = [[[NSFileManager defaultManager] attributesOfItemAtPath:path error:nil]
        objectForKey:NSFileSize];
    if (fileSize != nil) size = fileSize.unsignedLongLongValue;
    uint64_t duration = 0;
    if (self.movieOutput != nil && CMTIME_IS_VALID(self.movieOutput.recordedDuration)) {
        double seconds = CMTimeGetSeconds(self.movieOutput.recordedDuration);
        if (isfinite(seconds) && seconds > 0.0) duration = (uint64_t)(seconds * 1000000000.0);
    }

    OxideCamRecordEvent event = {
        .kind = self.recordCancelled ? 1u : (error == nil ? 0u : 2u),
        .path_ptr = pathData.bytes,
        .path_len = pathData.length,
        .duration_ns = duration,
        .size_bytes = size,
        .had_audio = self.recordHadAudio ? 1 : 0,
        .error_code = error == nil ? 0 : 7,
        .error_msg_ptr = NULL,
        .error_msg_len = 0,
    };
    NSData *errorData = nil;
    if (error != nil) {
        NSString *message = error.localizedDescription ?: @"camera recording failed";
        errorData = [message dataUsingEncoding:NSUTF8StringEncoding];
        event.error_msg_ptr = errorData.bytes;
        event.error_msg_len = errorData.length;
    }
    gOxideCameraRecordCallback(&event);
}

@end

static OxideMacCameraController *MacCameraController(void)
{
    static OxideMacCameraController *controller = nil;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        controller = [[OxideMacCameraController alloc] init];
    });
    return controller;
}

void oxide_host_set_camera_callback(OxideCameraFrameCallback callback)
{
    gOxideCameraFrameCallback = callback;
}

void oxide_host_set_camera_audio_callback(OxideCameraAudioCallback callback)
{
    gOxideCameraAudioCallback = callback;
}

void oxide_host_set_camera_record_callback(OxideCameraRecordCallback callback)
{
    gOxideCameraRecordCallback = callback;
}

void oxide_host_set_camera_photo_callback(OxideCameraPhotoCallback callback)
{
    gOxideCameraPhotoCallback = callback;
}

int32_t oxide_cam_start_default(void)
{
    __block int32_t result = OxideCamErrUnsupported;
    MacCameraSync(^{
        result = [MacCameraController() startDeliverFrames:YES enableAudio:YES];
    });
    return result;
}

int32_t oxide_cam_start_default_preview_only(void)
{
    __block int32_t result = OxideCamErrUnsupported;
    MacCameraSync(^{
        result = [MacCameraController() startDeliverFrames:YES enableAudio:NO];
    });
    return result;
}

int32_t oxide_cam_start_native_preview_layer(void)
{
    return OxideCamErrUnsupported;
}

void oxide_cam_stop(void)
{
    MacCameraSync(^{
        [MacCameraController() stop];
    });
}

int32_t oxide_cam_set_fps(int32_t fps)
{
    if (fps <= 0) return OxideCamErrInvalid;
    MacCameraSync(^{
        MacCameraController().fps = (uint32_t)fps;
    });
    return 0;
}

static uint32_t MacCameraWidthForHeight(uint32_t height)
{
    if (height == 0) return 1920;
    uint64_t width = ((uint64_t)height * 16ull) / 9ull;
    if (width == 0) return 1;
    if (width > UINT32_MAX) return UINT32_MAX;
    return (uint32_t)width;
}

int32_t oxide_cam_set_resolution_height(int32_t height)
{
    if (height <= 0) return OxideCamErrInvalid;
    MacCameraSync(^{
        MacCameraController().height = (uint32_t)height;
        MacCameraController().width = MacCameraWidthForHeight((uint32_t)height);
    });
    return 0;
}

int32_t oxide_cam_set_bit_depth(int32_t bits)
{
    MacCameraSync(^{
        MacCameraController().bitDepth = bits >= 10 ? 10 : 8;
    });
    return 0;
}

int32_t oxide_cam_set_color_space(int32_t colorSpace)
{
    MacCameraSync(^{
        MacCameraController().colorSpace = colorSpace;
    });
    return 0;
}

int32_t oxide_cam_set_position(int32_t position)
{
    MacCameraSync(^{
        MacCameraController().position = position == 1 ? 1 : 0;
    });
    return 0;
}

int32_t oxide_cam_set_mode(int32_t mode)
{
    if (mode < 0 || mode > 2) return OxideCamErrInvalid;
    MacCameraSync(^{
        MacCameraController().mode = mode;
    });
    return 0;
}

int32_t oxide_cam_set_focus_point(float x, float y)
{
    __block int32_t result = OxideCamErrUnsupported;
    MacCameraSync(^{
        result = [MacCameraController() setFocusX:x y:y];
    });
    return result;
}

int32_t oxide_cam_set_zoom_factor(float factor)
{
    (void)factor;
    return OxideCamErrUnsupported;
}

int32_t oxide_cam_set_flash_mode(int32_t mode)
{
    if (mode < 0 || mode > 2) return OxideCamErrInvalid;
    return mode == 0 ? 0 : OxideCamErrUnsupported;
}

int32_t oxide_cam_set_torch_mode(int32_t mode, float level)
{
    (void)level;
    if (mode < 0 || mode > 1) return OxideCamErrInvalid;
    return mode == 0 ? 0 : OxideCamErrUnsupported;
}

int32_t oxide_cam_capture_photo(uint8_t high_speed_from_preview, int32_t flash_mode)
{
    __block int32_t result = OxideCamErrUnsupported;
    MacCameraSync(^{
        result = [MacCameraController() capturePhotoHighSpeed:high_speed_from_preview != 0
                                                    flashMode:flash_mode];
    });
    return result;
}

int32_t oxide_cam_set_audio_session_mode(int32_t mode)
{
    (void)mode;
    return 0;
}

int32_t oxide_cam_record_start(const uint8_t *path_ptr, size_t path_len, int32_t container, uint8_t include_audio)
{
    __block int32_t result = OxideCamErrUnsupported;
    MacCameraSync(^{
        result = [MacCameraController() startRecordingPath:path_ptr
                                                       len:path_len
                                                 container:container
                                              includeAudio:include_audio != 0];
    });
    return result;
}

int32_t oxide_cam_record_stop(void)
{
    MacCameraSync(^{
        [MacCameraController().movieOutput stopRecording];
    });
    return 0;
}

int32_t oxide_cam_record_cancel(void)
{
    MacCameraSync(^{
        MacCameraController().recordCancelled = YES;
        [MacCameraController().movieOutput stopRecording];
    });
    return 0;
}

int32_t oxide_host_set_camera_running(uint8_t on)
{
    gOxideCameraRunning = on != 0 ? 1 : 0;
    return 0;
}

int32_t oxide_cam_query_formats(void **out_ptr, size_t *out_count)
{
    if (out_ptr == NULL || out_count == NULL) return 0;
    *out_ptr = NULL;
    *out_count = 0;
    AVCaptureDevice *device = MacCameraDeviceForPosition(0);
    if (device == nil) return 0;
    NSMutableData *data = [[NSMutableData alloc] init];
    for (AVCaptureDeviceFormat *format in device.formats) {
        CMVideoDimensions dims = CMVideoFormatDescriptionGetDimensions(format.formatDescription);
        if (dims.width <= 0 || dims.height <= 0) continue;
        for (AVFrameRateRange *range in format.videoSupportedFrameRateRanges) {
            OxideCamFormat row = {
                .width = dims.width,
                .height = dims.height,
                .fps_min = (float)range.minFrameRate,
                .fps_max = (float)range.maxFrameRate,
                .color_spaces_mask = 1u,
            };
            [data appendBytes:&row length:sizeof(row)];
        }
    }
    if (data.length == 0) return 0;
    void *copy = malloc(data.length);
    if (copy == NULL) return 0;
    memcpy(copy, data.bytes, data.length);
    *out_ptr = copy;
    *out_count = data.length / sizeof(OxideCamFormat);
    return 1;
}

int32_t oxide_cam_query_pixfmts(void **out_ptr, size_t *out_count)
{
    if (out_ptr == NULL || out_count == NULL) return 0;
    *out_ptr = NULL;
    *out_count = 0;
    OxideCamPixFmt *row = malloc(sizeof(OxideCamPixFmt));
    if (row == NULL) return 0;
    *row = (OxideCamPixFmt) {
        .code = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
        .bit_depth = 8,
        .range = 1,
    };
    *out_ptr = row;
    *out_count = 1;
    return 1;
}

void oxide_cam_caps_free(void *p)
{
    free(p);
}

typedef struct {
    const uint8_t *identifier_ptr;
    size_t identifier_len;
    uint8_t media_type;
    uint64_t creation_date;
    double duration_sec;
    uint32_t width;
    uint32_t height;
    uint64_t file_size;
} OxideMediaAsset;

typedef struct {
    const uint8_t *data_ptr;
    size_t data_len;
    uint32_t width;
    uint32_t height;
    size_t row_bytes;
} OxideImageData;

enum {
    OxideMediaErrPermissionDenied = -1,
    OxideMediaErrIo = -2,
    OxideMediaErrInvalid = -3,
    OxideMediaErrUnsupported = -4,
    OxideMediaNotFound = 1,
};

typedef void (*OxideWebViewEventCallback)(uint64_t view_id,
                                          uint32_t event_kind,
                                          const uint8_t *message_ptr,
                                          size_t message_len);

@class OxideMacLocationDelegate;
@class OxideMacWebViewDelegate;

static void MacEmitWebViewEvent(uint64_t view_id, uint32_t event_kind, NSString *message);
static NSString *MacStringFromBytes(const uint8_t *ptr, size_t len);
static UNUserNotificationCenter *MacNotificationCenter(void);

static void (*gOxideLocationCallback)(const OxideLocationSample *) = NULL;
static void (*gOxideLocationErrorCallback)(const uint8_t *, size_t) = NULL;
static CLLocationManager *gOxideLocationManager = nil;
static OxideMacLocationDelegate *gOxideLocationDelegate = nil;
static OxideLocationSample gOxideLastLocation;
static BOOL gOxideHasLastLocation = NO;
static BOOL gOxideMotionRunning = NO;
static void (*gOxidePushTokenCallback)(uint32_t provider, const uint8_t *utf8, size_t len) = NULL;
static void (*gOxidePushNotifyCallback)(const uint8_t *utf8, size_t len) = NULL;
static NSString *gOxidePushDeviceToken = nil;
static OxideWebViewEventCallback gOxideWebViewEventCallback = NULL;
static NSMutableDictionary<NSNumber *, WKWebView *> *gOxideWebViews = nil;
static NSMutableDictionary<NSNumber *, OxideMacWebViewDelegate *> *gOxideWebViewDelegates = nil;

@interface OxideMacLocationDelegate : NSObject <CLLocationManagerDelegate>
@end

@implementation OxideMacLocationDelegate
- (void)locationManager:(CLLocationManager *)manager
     didUpdateLocations:(NSArray<CLLocation *> *)locations
{
    (void)manager;
    CLLocation *latest = locations.lastObject;
    if (latest == nil) return;

    OxideLocationSample sample;
    sample.latitude = latest.coordinate.latitude;
    sample.longitude = latest.coordinate.longitude;
    sample.altitude = latest.altitude;
    sample.horizontal_accuracy = latest.horizontalAccuracy;
    sample.vertical_accuracy = latest.verticalAccuracy;
    sample.speed = latest.speed < 0 ? 0 : latest.speed;
    sample.course = latest.course < 0 ? 0 : latest.course;
    sample.timestamp_ms = (uint64_t)(latest.timestamp.timeIntervalSince1970 * 1000.0);

    gOxideLastLocation = sample;
    gOxideHasLastLocation = YES;

    if (gOxideLocationCallback != NULL) {
        gOxideLocationCallback(&sample);
    }
}

- (void)locationManager:(CLLocationManager *)manager
       didFailWithError:(NSError *)error
{
    (void)manager;
    if (gOxideLocationErrorCallback == NULL) return;
    const char *msg = error.localizedDescription.UTF8String ?: "unknown location error";
    gOxideLocationErrorCallback((const uint8_t *)msg, strlen(msg));
}

- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager
{
    (void)manager;
    MacEmitPermission(1u, macos_permission_status(1u));
}

- (void)locationManager:(CLLocationManager *)manager didChangeAuthorizationStatus:(CLAuthorizationStatus)status
{
    (void)manager;
    (void)status;
    MacEmitPermission(1u, macos_permission_status(1u));
}
@end

@interface OxideMacWebViewDelegate : NSObject <WKNavigationDelegate>
@property (nonatomic) uint64_t viewId;
@end

@implementation OxideMacWebViewDelegate
- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation
{
    (void)webView;
    (void)navigation;
    MacEmitWebViewEvent(self.viewId, 0, nil);
}

- (void)webView:(WKWebView *)webView
didFailProvisionalNavigation:(WKNavigation *)navigation
      withError:(NSError *)error
{
    (void)webView;
    (void)navigation;
    MacEmitWebViewEvent(self.viewId, 1, error.localizedDescription);
}

- (void)webView:(WKWebView *)webView
didFailNavigation:(WKNavigation *)navigation
      withError:(NSError *)error
{
    (void)webView;
    (void)navigation;
    MacEmitWebViewEvent(self.viewId, 1, error.localizedDescription);
}
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
    UNUserNotificationCenter *notificationCenter = MacNotificationCenter();
    if (notificationCenter != nil) {
        notificationCenter.delegate = self;
    }
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

- (void)application:(NSApplication *)application
didRegisterForRemoteNotificationsWithDeviceToken:(NSData *)deviceToken
{
    (void)application;
    MacEmitPushToken(MacHexToken(deviceToken));
}

- (void)application:(NSApplication *)application
didFailToRegisterForRemoteNotificationsWithError:(NSError *)error
{
    (void)application;
    (void)error;
    gOxidePushDeviceToken = nil;
    if (gOxidePushTokenCallback != NULL) {
        gOxidePushTokenCallback(0, NULL, 0);
    }
}

- (void)application:(NSApplication *)application didReceiveRemoteNotification:(NSDictionary<NSString *, id> *)userInfo
{
    (void)application;
    MacEmitPushNotification(userInfo);
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       willPresentNotification:(UNNotification *)notification
         withCompletionHandler:(void (^)(UNNotificationPresentationOptions options))completionHandler
{
    (void)center;
    MacEmitPushNotification(notification.request.content.userInfo);
    if (completionHandler != nil) {
        completionHandler(UNNotificationPresentationOptionList |
                          UNNotificationPresentationOptionBanner |
                          UNNotificationPresentationOptionSound |
                          UNNotificationPresentationOptionBadge);
    }
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
didReceiveNotificationResponse:(UNNotificationResponse *)response
         withCompletionHandler:(void (^)(void))completionHandler
{
    (void)center;
    MacEmitPushNotification(response.notification.request.content.userInfo);
    if (completionHandler != nil) {
        completionHandler();
    }
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
    if (!out_ptr || !out_len) return 0;
    *out_ptr = NULL;
    *out_len = 0;
    NSPasteboard *pb = [NSPasteboard generalPasteboard];
    NSString *s = [pb stringForType:NSPasteboardTypeString];
    if (!s) return 0;
    NSData *d = [s dataUsingEncoding:NSUTF8StringEncoding];
    if (!d) return 0;
    if (d.length == 0) return 1;
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
    dispatch_async(dispatch_get_main_queue(), ^{
        NSHapticFeedbackPattern feedback = NSHapticFeedbackPatternGeneric;
        switch (pattern) {
            case 0:
            case 1:
            case 2:
                feedback = NSHapticFeedbackPatternAlignment;
                break;
            case 3:
                feedback = NSHapticFeedbackPatternLevelChange;
                break;
            case 4:
            case 5:
            case 6:
            default:
                feedback = NSHapticFeedbackPatternGeneric;
                break;
        }
        [[NSHapticFeedbackManager defaultPerformer] performFeedbackPattern:feedback
                                                           performanceTime:NSHapticFeedbackPerformanceTimeNow];
    });
}

void macos_open_system_settings(void)
{
    dispatch_async(dispatch_get_main_queue(), ^{
        NSURL *url = [NSURL URLWithString:@"x-apple.systempreferences:"];
        if (url) {
            [[NSWorkspace sharedWorkspace] openURL:url];
        }
    });
}

int macos_open_external_url(const char *utf8, size_t len)
{
    if (!utf8 || len == 0) return 0;
    NSString *s = [[NSString alloc] initWithBytes:utf8 length:len encoding:NSUTF8StringEncoding];
    if (!s) return 0;
    NSURL *url = [NSURL URLWithString:s];
    if (!url) return 0;
    __block BOOL ok = NO;
    void (^openBlock)(void) = ^{
        ok = [[NSWorkspace sharedWorkspace] openURL:url];
    };
    if ([NSThread isMainThread]) {
        openBlock();
    } else {
        dispatch_sync(dispatch_get_main_queue(), openBlock);
    }
    return ok ? 1 : 0;
}

static NSScreen *CurrentOxideScreen(void)
{
    NSView *view = gMetalView;
    if (view.window.screen) return view.window.screen;
    return NSScreen.mainScreen;
}

uint32_t macos_max_framerate_hz(void)
{
    NSScreen *screen = CurrentOxideScreen();
    NSInteger hz = screen.maximumFramesPerSecond;
    return hz > 0 ? (uint32_t)hz : 60u;
}

float macos_native_scale(void)
{
    NSScreen *screen = CurrentOxideScreen();
    CGFloat scale = screen.backingScaleFactor;
    return scale > 0.0 ? (float)scale : 1.0f;
}

uint8_t macos_supports_edr(void)
{
    NSScreen *screen = CurrentOxideScreen();
    if (@available(macOS 10.15, *)) {
        if ([screen respondsToSelector:@selector(maximumPotentialExtendedDynamicRangeColorComponentValue)]) {
            return screen.maximumPotentialExtendedDynamicRangeColorComponentValue > 1.0 ? 1 : 0;
        }
    }
    return 0;
}

uint8_t macos_reduce_motion_enabled(void)
{
    NSWorkspace *workspace = [NSWorkspace sharedWorkspace];
    if ([workspace respondsToSelector:@selector(accessibilityDisplayShouldReduceMotion)]) {
        return workspace.accessibilityDisplayShouldReduceMotion ? 1 : 0;
    }
    return 0;
}

uint8_t macos_camera_available(void)
{
    return MacCameraDeviceForPosition(0) != nil ? 1 : 0;
}

typedef void (*OxideNetworkStatusCallback)(uint8_t connected, uint32_t interfaces);

#define kOxideNetworkInterfaceWifi (1u << 0)
#define kOxideNetworkInterfaceCellular (1u << 1)
#define kOxideNetworkInterfaceWired (1u << 2)
static dispatch_once_t gOxideNetworkMonitorOnce;
static nw_path_monitor_t gOxideNetworkMonitor = nil;
static int gOxideNetworkConnected = 0;
static uint32_t gOxideNetworkInterfaces = 0;
static OxideNetworkStatusCallback gOxideNetworkStatusCallback = NULL;

static uint32_t OxideNetworkInterfacesForPath(nw_path_t path)
{
    __block uint32_t interfaces = 0;
    if (path == NULL) return interfaces;
    nw_path_enumerate_interfaces(path, ^bool(nw_interface_t interface) {
        if (interface == NULL) return true;
        nw_interface_type_t type = nw_interface_get_type(interface);
        if (type == nw_interface_type_wifi) {
            interfaces |= kOxideNetworkInterfaceWifi;
        } else if (type == nw_interface_type_cellular) {
            interfaces |= kOxideNetworkInterfaceCellular;
        } else if (type == nw_interface_type_wired) {
            interfaces |= kOxideNetworkInterfaceWired;
        }
        return true;
    });
    return interfaces;
}

static void OxideNetworkPublishPath(nw_path_t path)
{
    int connected = nw_path_get_status(path) == nw_path_status_satisfied ? 1 : 0;
    uint32_t interfaces = connected ? OxideNetworkInterfacesForPath(path) : 0;
    __atomic_store_n(&gOxideNetworkConnected, connected, __ATOMIC_RELAXED);
    __atomic_store_n(&gOxideNetworkInterfaces, interfaces, __ATOMIC_RELAXED);
    OxideNetworkStatusCallback callback = gOxideNetworkStatusCallback;
    if (callback != NULL) {
        callback(connected ? 1 : 0, interfaces);
    }
}

int32_t macos_start_network_monitor(void)
{
    dispatch_once(&gOxideNetworkMonitorOnce, ^{
        gOxideNetworkMonitor = nw_path_monitor_create();
        if (gOxideNetworkMonitor == nil) return;
        nw_path_monitor_set_queue(gOxideNetworkMonitor, dispatch_get_global_queue(QOS_CLASS_UTILITY, 0));
        nw_path_monitor_set_update_handler(gOxideNetworkMonitor, ^(nw_path_t path) {
            OxideNetworkPublishPath(path);
        });
        nw_path_monitor_start(gOxideNetworkMonitor);
    });
    return gOxideNetworkMonitor == nil ? 0 : 1;
}

int32_t macos_network_status(uint8_t *out_connected, uint32_t *out_interfaces)
{
    if (out_connected == NULL || out_interfaces == NULL) return 0;
    if (macos_start_network_monitor() == 0) return 0;
    *out_connected = __atomic_load_n(&gOxideNetworkConnected, __ATOMIC_RELAXED) ? 1 : 0;
    *out_interfaces = __atomic_load_n(&gOxideNetworkInterfaces, __ATOMIC_RELAXED);
    return 1;
}

void macos_set_network_status_callback(OxideNetworkStatusCallback callback)
{
    gOxideNetworkStatusCallback = callback;
    if (callback != NULL) {
        callback(__atomic_load_n(&gOxideNetworkConnected, __ATOMIC_RELAXED) ? 1 : 0,
                 __atomic_load_n(&gOxideNetworkInterfaces, __ATOMIC_RELAXED));
    }
}

typedef void (*OxidePermissionCallback)(uint32_t domain, uint32_t status);

enum {
    kOxidePermissionNotDetermined = 0,
    kOxidePermissionDenied = 1,
    kOxidePermissionLimited = 2,
    kOxidePermissionAuthorized = 3,
};

enum {
    kOxidePermissionNotifications = 0,
    kOxidePermissionLocation = 1,
    kOxidePermissionCamera = 2,
    kOxidePermissionContacts = 3,
    kOxidePermissionBluetooth = 4,
    kOxidePermissionMotion = 5,
    kOxidePermissionMicrophone = 6,
    kOxidePermissionMediaLibrary = 7,
};

static OxidePermissionCallback gOxidePermissionCallback = NULL;
static CLLocationManager *gOxidePermissionLocationManager = nil;

static void MacDispatchMainAsync(void (^block)(void))
{
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_async(dispatch_get_main_queue(), block);
    }
}

static void MacDispatchMainSync(void (^block)(void))
{
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_sync(dispatch_get_main_queue(), block);
    }
}

static NSMutableDictionary<NSNumber *, WKWebView *> *MacWebViews(void)
{
    if (gOxideWebViews == nil) {
        gOxideWebViews = [[NSMutableDictionary alloc] init];
    }
    return gOxideWebViews;
}

static NSMutableDictionary<NSNumber *, OxideMacWebViewDelegate *> *MacWebViewDelegates(void)
{
    if (gOxideWebViewDelegates == nil) {
        gOxideWebViewDelegates = [[NSMutableDictionary alloc] init];
    }
    return gOxideWebViewDelegates;
}

static NSString *MacStringFromJavaScriptResult(id result)
{
    if (result == nil || result == [NSNull null]) return nil;
    if ([result isKindOfClass:[NSString class]]) return (NSString *)result;
    if ([result isKindOfClass:[NSNumber class]]) return [(NSNumber *)result stringValue];
    if ([NSJSONSerialization isValidJSONObject:result]) {
        NSError *error = nil;
        NSData *json = [NSJSONSerialization dataWithJSONObject:result options:0 error:&error];
        if (json != nil && error == nil) {
            return [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
        }
    }
    return [result description];
}

static uint8_t *MacCopyUtf8(NSString *value, size_t *out_len)
{
    if (out_len != NULL) *out_len = 0;
    if (value.length == 0) return NULL;
    NSData *utf8 = [value dataUsingEncoding:NSUTF8StringEncoding];
    if (utf8 == nil || utf8.length == 0) return NULL;
    uint8_t *copy = malloc(utf8.length);
    if (copy == NULL) return NULL;
    memcpy(copy, utf8.bytes, utf8.length);
    if (out_len != NULL) *out_len = (size_t)utf8.length;
    return copy;
}

static void MacEmitWebViewEvent(uint64_t view_id, uint32_t event_kind, NSString *message)
{
    OxideWebViewEventCallback callback = gOxideWebViewEventCallback;
    if (callback == NULL) return;
    NSData *utf8 = [message dataUsingEncoding:NSUTF8StringEncoding];
    callback(view_id,
             event_kind,
             utf8 != nil ? utf8.bytes : NULL,
             utf8 != nil ? utf8.length : 0);
}

static CLLocationManager *MacLocationManager(void)
{
    __block CLLocationManager *manager = nil;
    MacDispatchMainSync(^{
        if (gOxideLocationManager == nil) {
            gOxideLocationDelegate = [[OxideMacLocationDelegate alloc] init];
            gOxideLocationManager = [[CLLocationManager alloc] init];
            gOxideLocationManager.delegate = gOxideLocationDelegate;
        }
        manager = gOxideLocationManager;
    });
    return manager;
}

static CLLocationAccuracy MacDesiredLocationAccuracy(uint32_t accuracy_kind)
{
    switch (accuracy_kind) {
        case 0:
        case 2:
            return kCLLocationAccuracyThreeKilometers;
        case 1:
            return kCLLocationAccuracyHundredMeters;
        case 3:
        default:
            return kCLLocationAccuracyBest;
    }
}

static void MacApplyLocationConfig(CLLocationManager *manager, OxideLocationConfig cfg)
{
    (void)cfg.allow_background;
    (void)cfg.precise;
    manager.desiredAccuracy = MacDesiredLocationAccuracy(cfg.accuracy_kind);
    manager.distanceFilter = cfg.distance_filter_m > 0 ? cfg.distance_filter_m : kCLDistanceFilterNone;
}

static NSString *MacHexToken(NSData *deviceToken)
{
    if (deviceToken == nil || deviceToken.length == 0) return nil;
    const unsigned char *bytes = deviceToken.bytes;
    NSMutableString *token = [NSMutableString stringWithCapacity:deviceToken.length * 2];
    for (NSUInteger i = 0; i < deviceToken.length; i += 1) {
        [token appendFormat:@"%02x", bytes[i]];
    }
    return token;
}

static void MacEmitPushToken(NSString *token)
{
    gOxidePushDeviceToken = token;
    if (gOxidePushTokenCallback == NULL || token.length == 0) return;
    NSData *utf8 = [token dataUsingEncoding:NSUTF8StringEncoding];
    if (utf8 == nil) return;
    gOxidePushTokenCallback(0, utf8.bytes, utf8.length);
}

static void MacEmitPushNotification(NSDictionary *userInfo)
{
    if (gOxidePushNotifyCallback == NULL || userInfo == nil) return;
    if (![NSJSONSerialization isValidJSONObject:userInfo]) return;
    NSError *error = nil;
    NSData *data = [NSJSONSerialization dataWithJSONObject:userInfo options:0 error:&error];
    if (data == nil || error != nil) return;
    gOxidePushNotifyCallback(data.bytes, data.length);
}

uint8_t macos_location_services_available(void)
{
    return [CLLocationManager locationServicesEnabled] ? 1 : 0;
}

void oxide_host_set_location_callback(void (*callback)(const OxideLocationSample *))
{
    gOxideLocationCallback = callback;
}

void oxide_host_set_location_error_callback(void (*callback)(const uint8_t *, size_t))
{
    gOxideLocationErrorCallback = callback;
}

int32_t oxide_host_location_start(OxideLocationConfig cfg)
{
    __block int32_t result = 0;
    MacDispatchMainSync(^{
        if (![CLLocationManager locationServicesEnabled]) {
            result = 1;
            return;
        }
        CLLocationManager *manager = MacLocationManager();
        MacApplyLocationConfig(manager, cfg);
        CLAuthorizationStatus status = manager.authorizationStatus;
        if (status == kCLAuthorizationStatusNotDetermined) {
            [manager requestWhenInUseAuthorization];
        }
        [manager startUpdatingLocation];
    });
    return result;
}

void oxide_host_location_stop(void)
{
    MacDispatchMainAsync(^{
        if (gOxideLocationManager != nil) {
            [gOxideLocationManager stopUpdatingLocation];
        }
    });
}

void oxide_host_location_request_once(void)
{
    MacDispatchMainAsync(^{
        CLLocationManager *manager = MacLocationManager();
        [manager requestLocation];
    });
}

uint8_t oxide_host_location_last(OxideLocationSample *out_ptr)
{
    if (out_ptr == NULL) return 0;
    __block uint8_t has_sample = 0;
    MacDispatchMainSync(^{
        if (!gOxideHasLastLocation) return;
        *out_ptr = gOxideLastLocation;
        has_sample = 1;
    });
    return has_sample;
}

int32_t oxide_host_location_set_accuracy(uint32_t accuracy_kind)
{
    MacDispatchMainSync(^{
        CLLocationManager *manager = MacLocationManager();
        manager.desiredAccuracy = MacDesiredLocationAccuracy(accuracy_kind);
    });
    return 0;
}

uint8_t macos_motion_available(void)
{
    return 0;
}

void oxide_host_set_motion_callback(void (*callback)(const OxideMotionSample *))
{
    (void)callback;
}

int32_t oxide_host_motion_start(void)
{
    return 1;
}

void oxide_host_motion_stop(void)
{
    gOxideMotionRunning = NO;
}

uint8_t oxide_host_motion_is_active(void)
{
    return gOxideMotionRunning ? 1 : 0;
}

static void MacEmitPermission(uint32_t domain, uint32_t status)
{
    OxidePermissionCallback callback = gOxidePermissionCallback;
    if (callback != NULL) {
        callback(domain, status);
    }
}

void oxide_host_emit_perm(uint32_t domain, uint32_t status)
{
    MacEmitPermission(domain, status);
}

static OxideMacLocationPermissionDelegate *gOxidePermissionLocationDelegate = nil;

static CLLocationManager *MacPermissionLocationManager(void)
{
    __block CLLocationManager *manager = nil;
    MacDispatchMainSync(^{
        static dispatch_once_t once;
        dispatch_once(&once, ^{
            gOxidePermissionLocationDelegate = [[OxideMacLocationPermissionDelegate alloc] init];
            gOxidePermissionLocationManager = [[CLLocationManager alloc] init];
            gOxidePermissionLocationManager.delegate = gOxidePermissionLocationDelegate;
        });
        manager = gOxidePermissionLocationManager;
    });
    return manager;
}

static uint32_t MacPermissionFromAV(AVAuthorizationStatus status)
{
    switch (status) {
        case AVAuthorizationStatusAuthorized:
            return kOxidePermissionAuthorized;
        case AVAuthorizationStatusDenied:
        case AVAuthorizationStatusRestricted:
            return kOxidePermissionDenied;
        case AVAuthorizationStatusNotDetermined:
        default:
            return kOxidePermissionNotDetermined;
    }
}

static uint32_t MacPermissionFromContacts(CNAuthorizationStatus status)
{
    switch (status) {
        case CNAuthorizationStatusAuthorized:
            return kOxidePermissionAuthorized;
        case CNAuthorizationStatusDenied:
        case CNAuthorizationStatusRestricted:
            return kOxidePermissionDenied;
        case CNAuthorizationStatusNotDetermined:
        default:
            return kOxidePermissionNotDetermined;
    }
}

static uint32_t MacPermissionFromLocation(CLAuthorizationStatus status)
{
    switch (status) {
        case kCLAuthorizationStatusAuthorizedAlways:
            return kOxidePermissionAuthorized;
        case kCLAuthorizationStatusDenied:
        case kCLAuthorizationStatusRestricted:
            return kOxidePermissionDenied;
        case kCLAuthorizationStatusNotDetermined:
        default:
            return kOxidePermissionNotDetermined;
    }
}

static uint32_t MacPermissionFromPhotos(PHAuthorizationStatus status)
{
    switch (status) {
        case PHAuthorizationStatusAuthorized:
            return kOxidePermissionAuthorized;
#if __MAC_OS_X_VERSION_MAX_ALLOWED >= 110000
        case PHAuthorizationStatusLimited:
            return kOxidePermissionLimited;
#endif
        case PHAuthorizationStatusDenied:
        case PHAuthorizationStatusRestricted:
            return kOxidePermissionDenied;
        case PHAuthorizationStatusNotDetermined:
        default:
            return kOxidePermissionNotDetermined;
    }
}

static uint32_t MacPermissionFromNotifications(UNAuthorizationStatus status)
{
    switch (status) {
        case UNAuthorizationStatusAuthorized:
        case UNAuthorizationStatusProvisional:
            return kOxidePermissionAuthorized;
        case UNAuthorizationStatusDenied:
            return kOxidePermissionDenied;
        case UNAuthorizationStatusNotDetermined:
        default:
            return kOxidePermissionNotDetermined;
    }
}

static UNUserNotificationCenter *MacNotificationCenter(void)
{
    if (NSBundle.mainBundle.bundleIdentifier.length == 0) return nil;
    @try {
        return [UNUserNotificationCenter currentNotificationCenter];
    } @catch (NSException *exception) {
        (void)exception;
        return nil;
    }
}

static uint32_t MacNotificationPermissionStatus(void)
{
    UNUserNotificationCenter *center = MacNotificationCenter();
    if (center == nil) return kOxidePermissionNotDetermined;
    __block uint32_t status = kOxidePermissionNotDetermined;
    dispatch_semaphore_t sem = dispatch_semaphore_create(0);
    [center getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings) {
        status = MacPermissionFromNotifications(settings.authorizationStatus);
        dispatch_semaphore_signal(sem);
    }];
    dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, (int64_t)(2 * NSEC_PER_SEC));
    if (dispatch_semaphore_wait(sem, timeout) != 0) {
        return kOxidePermissionNotDetermined;
    }
    return status;
}

static uint32_t MacLocationPermissionStatus(void)
{
    if (![CLLocationManager locationServicesEnabled]) return kOxidePermissionDenied;
    __block uint32_t status = kOxidePermissionNotDetermined;
    MacDispatchMainSync(^{
        CLLocationManager *manager = MacPermissionLocationManager();
        if (@available(macOS 11.0, *)) {
            status = MacPermissionFromLocation(manager.authorizationStatus);
        }
    });
    return status;
}

static uint32_t MacBluetoothPermissionStatus(void)
{
    if (@available(macOS 10.15, *)) {
        switch (CBManager.authorization) {
            case CBManagerAuthorizationAllowedAlways:
                return kOxidePermissionAuthorized;
            case CBManagerAuthorizationDenied:
            case CBManagerAuthorizationRestricted:
                return kOxidePermissionDenied;
            case CBManagerAuthorizationNotDetermined:
            default:
                return kOxidePermissionNotDetermined;
        }
    }
    return kOxidePermissionDenied;
}

static uint32_t MacMediaLibraryPermissionStatus(void)
{
    if (@available(macOS 11.0, *)) {
        return MacPermissionFromPhotos([PHPhotoLibrary authorizationStatusForAccessLevel:PHAccessLevelReadWrite]);
    }
    return kOxidePermissionNotDetermined;
}

uint32_t macos_permission_status(uint32_t domain)
{
    switch (domain) {
        case kOxidePermissionNotifications:
            return MacNotificationPermissionStatus();
        case kOxidePermissionLocation:
            return MacLocationPermissionStatus();
        case kOxidePermissionCamera:
            return MacPermissionFromAV([AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo]);
        case kOxidePermissionContacts:
            return MacPermissionFromContacts([CNContactStore authorizationStatusForEntityType:CNEntityTypeContacts]);
        case kOxidePermissionBluetooth:
            return MacBluetoothPermissionStatus();
        case kOxidePermissionMotion:
            return kOxidePermissionDenied;
        case kOxidePermissionMicrophone:
            return MacPermissionFromAV([AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]);
        case kOxidePermissionMediaLibrary:
            return MacMediaLibraryPermissionStatus();
        default:
            return kOxidePermissionDenied;
    }
}

void macos_permission_request(uint32_t domain)
{
    switch (domain) {
        case kOxidePermissionNotifications: {
            UNUserNotificationCenter *center = MacNotificationCenter();
            if (center == nil) {
                MacEmitPermission(kOxidePermissionNotifications, kOxidePermissionNotDetermined);
                break;
            }
            UNAuthorizationOptions options = UNAuthorizationOptionAlert |
                                             UNAuthorizationOptionBadge |
                                             UNAuthorizationOptionSound;
            [center requestAuthorizationWithOptions:options
                                  completionHandler:^(BOOL granted, NSError *error) {
                                      (void)granted;
                                      (void)error;
                                      MacEmitPermission(kOxidePermissionNotifications,
                                                        MacNotificationPermissionStatus());
                                  }];
            break;
        }
        case kOxidePermissionLocation:
            MacDispatchMainAsync(^{
                [MacPermissionLocationManager() requestWhenInUseAuthorization];
            });
            MacEmitPermission(kOxidePermissionLocation, MacLocationPermissionStatus());
            break;
        case kOxidePermissionCamera:
            [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL granted) {
                (void)granted;
                MacEmitPermission(kOxidePermissionCamera, macos_permission_status(kOxidePermissionCamera));
            }];
            break;
        case kOxidePermissionContacts: {
            CNContactStore *store = [[CNContactStore alloc] init];
            [store requestAccessForEntityType:CNEntityTypeContacts completionHandler:^(BOOL granted, NSError *error) {
                (void)granted;
                (void)error;
                MacEmitPermission(kOxidePermissionContacts, macos_permission_status(kOxidePermissionContacts));
            }];
            break;
        }
        case kOxidePermissionBluetooth:
            MacEmitPermission(kOxidePermissionBluetooth, MacBluetoothPermissionStatus());
            break;
        case kOxidePermissionMotion:
            MacEmitPermission(kOxidePermissionMotion, kOxidePermissionDenied);
            break;
        case kOxidePermissionMicrophone:
            [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) {
                (void)granted;
                MacEmitPermission(kOxidePermissionMicrophone, macos_permission_status(kOxidePermissionMicrophone));
            }];
            break;
        case kOxidePermissionMediaLibrary:
            if (@available(macOS 11.0, *)) {
                [PHPhotoLibrary requestAuthorizationForAccessLevel:PHAccessLevelReadWrite handler:^(PHAuthorizationStatus status) {
                    MacEmitPermission(kOxidePermissionMediaLibrary, MacPermissionFromPhotos(status));
                }];
            } else {
                MacEmitPermission(kOxidePermissionMediaLibrary, kOxidePermissionNotDetermined);
            }
            break;
        default:
            MacEmitPermission(domain, kOxidePermissionDenied);
            break;
    }
}

void macos_set_permission_callback(OxidePermissionCallback callback)
{
    gOxidePermissionCallback = callback;
}

void oxide_host_set_push_token_callback(void (*callback)(uint32_t, const uint8_t *, size_t))
{
    gOxidePushTokenCallback = callback;
}

void oxide_host_set_push_notify_callback(void (*callback)(const uint8_t *, size_t))
{
    gOxidePushNotifyCallback = callback;
}

void oxide_host_push_register(void)
{
    dispatch_async(dispatch_get_main_queue(), ^{
        UNUserNotificationCenter *center = MacNotificationCenter();
        if (center == nil) {
            MacEmitPermission(kOxidePermissionNotifications, kOxidePermissionNotDetermined);
            return;
        }
        UNAuthorizationOptions options = UNAuthorizationOptionAlert |
                                         UNAuthorizationOptionBadge |
                                         UNAuthorizationOptionSound;
        [center requestAuthorizationWithOptions:options
                              completionHandler:^(BOOL granted, NSError *error) {
                                  (void)error;
                                  MacEmitPermission(kOxidePermissionNotifications,
                                                    MacNotificationPermissionStatus());
                                  if (granted) {
                                      dispatch_async(dispatch_get_main_queue(), ^{
                                          [NSApp registerForRemoteNotifications];
                                      });
                                  }
                              }];
    });
}

int32_t oxide_host_push_get_device_token(uint8_t **out_ptr, size_t *out_len)
{
    if (out_ptr != NULL) *out_ptr = NULL;
    if (out_len != NULL) *out_len = 0;
    if (out_ptr == NULL || out_len == NULL) return 0;
    __block NSString *token = nil;
    MacDispatchMainSync(^{
        token = gOxidePushDeviceToken;
    });
    if (token.length == 0) return 0;
    NSData *utf8 = [token dataUsingEncoding:NSUTF8StringEncoding];
    if (utf8 == nil || utf8.length == 0) return 0;
    uint8_t *copy = malloc(utf8.length);
    if (copy == NULL) return 0;
    memcpy(copy, utf8.bytes, utf8.length);
    *out_ptr = copy;
    *out_len = (size_t)utf8.length;
    return 1;
}

void oxide_host_push_set_badge(int32_t count)
{
    dispatch_async(dispatch_get_main_queue(), ^{
        NSApp.dockTile.badgeLabel = count > 0 ? [NSString stringWithFormat:@"%d", count] : nil;
        [NSApp.dockTile display];
    });
}

void oxide_host_push_clear_badge(void)
{
    oxide_host_push_set_badge(0);
}

void oxide_host_push_clear_all_delivered(void)
{
    UNUserNotificationCenter *center = MacNotificationCenter();
    if (center != nil) {
        [center removeAllDeliveredNotifications];
    }
    oxide_host_push_clear_badge();
}

void oxide_web_view_set_event_callback(OxideWebViewEventCallback callback)
{
    gOxideWebViewEventCallback = callback;
}

int32_t oxide_web_view_create(const uint8_t *url_ptr, size_t url_len, uint64_t view_id)
{
    if (url_ptr == NULL || url_len == 0 || view_id == 0) return -1;
    NSString *urlString = MacStringFromBytes(url_ptr, url_len);
    if (urlString.length == 0) return -1;
    NSURL *url = [NSURL URLWithString:urlString];
    if (url == nil || url.scheme.length == 0) return -1;

    __block int32_t result = 0;
    MacDispatchMainSync(^{
        NSNumber *key = @(view_id);
        if (MacWebViews()[key] != nil) {
            result = -3;
            return;
        }
        WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
        WKWebView *webView = [[WKWebView alloc] initWithFrame:NSMakeRect(-10000, -10000, 1, 1)
                                                configuration:configuration];
        if (webView == nil) {
            result = -2;
            return;
        }
        OxideMacWebViewDelegate *delegate = [[OxideMacWebViewDelegate alloc] init];
        delegate.viewId = view_id;
        webView.navigationDelegate = delegate;
        webView.hidden = YES;
        if (gMetalView != nil) {
            [gMetalView addSubview:webView];
        }
        MacWebViews()[key] = webView;
        MacWebViewDelegates()[key] = delegate;
        [webView loadRequest:[NSURLRequest requestWithURL:url]];
    });

    return result;
}

int32_t oxide_web_view_execute_script(
    uint64_t view_id,
    const uint8_t *script_ptr,
    size_t script_len,
    uint8_t **out_ptr,
    size_t *out_len)
{
    if (out_ptr != NULL) *out_ptr = NULL;
    if (out_len != NULL) *out_len = 0;
    if (view_id == 0 || script_ptr == NULL || script_len == 0 ||
        out_ptr == NULL || out_len == NULL) {
        return -1;
    }
    NSString *script = MacStringFromBytes(script_ptr, script_len);
    if (script.length == 0) return -1;

    __block int32_t result = 0;
    __block NSString *value = nil;
    __block BOOL done = NO;
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    void (^evaluate)(void) = ^{
        WKWebView *webView = MacWebViews()[@(view_id)];
        if (webView == nil) {
            result = -4;
            done = YES;
            dispatch_semaphore_signal(semaphore);
            return;
        }
        [webView evaluateJavaScript:script completionHandler:^(id jsResult, NSError *error) {
            if (error != nil) {
                result = -5;
            } else {
                value = MacStringFromJavaScriptResult(jsResult);
            }
            done = YES;
            dispatch_semaphore_signal(semaphore);
        }];
    };

    if ([NSThread isMainThread]) {
        evaluate();
        while (!done) {
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                    beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
        }
    } else {
        dispatch_async(dispatch_get_main_queue(), evaluate);
        dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);
    }

    if (result != 0) return result;
    if (value == nil) return 0;
    if (value.length == 0) return 1;
    uint8_t *copy = MacCopyUtf8(value, out_len);
    if (copy == NULL) return -6;
    *out_ptr = copy;
    return 1;
}

void oxide_web_view_close(uint64_t view_id)
{
    if (view_id == 0) return;
    MacDispatchMainAsync(^{
        NSNumber *key = @(view_id);
        WKWebView *webView = MacWebViews()[key];
        if (webView != nil) {
            webView.navigationDelegate = nil;
            [webView stopLoading];
            [webView removeFromSuperview];
            [gOxideWebViews removeObjectForKey:key];
        }
        [gOxideWebViewDelegates removeObjectForKey:key];
    });
}

void oxide_web_view_free_string(uint8_t *data_ptr)
{
    if (data_ptr != NULL) free(data_ptr);
}

static BOOL MacMediaLibraryAuthorized(void)
{
    if (@available(macOS 11.0, *)) {
        PHAuthorizationStatus status =
            [PHPhotoLibrary authorizationStatusForAccessLevel:PHAccessLevelReadWrite];
        return status == PHAuthorizationStatusAuthorized || status == PHAuthorizationStatusLimited;
    }
    PHAuthorizationStatus status = [PHPhotoLibrary authorizationStatus];
    return status == PHAuthorizationStatusAuthorized;
}

static void MacZeroImageData(OxideImageData *image)
{
    if (image == NULL) return;
    image->data_ptr = NULL;
    image->data_len = 0;
    image->width = 0;
    image->height = 0;
    image->row_bytes = 0;
}

static BOOL MacCopyData(NSData *data, const uint8_t **out_ptr, size_t *out_len)
{
    if (out_ptr == NULL || out_len == NULL) return NO;
    *out_ptr = NULL;
    *out_len = 0;
    if (data == nil || data.length == 0) return YES;
    uint8_t *buffer = (uint8_t *)malloc(data.length);
    if (buffer == NULL) return NO;
    memcpy(buffer, data.bytes, data.length);
    *out_ptr = buffer;
    *out_len = (size_t)data.length;
    return YES;
}

static BOOL MacCopyString(NSString *string, const uint8_t **out_ptr, size_t *out_len)
{
    NSData *data = string == nil ? nil : [string dataUsingEncoding:NSUTF8StringEncoding];
    return MacCopyData(data, out_ptr, out_len);
}

static NSString *MacStringFromBytes(const uint8_t *ptr, size_t len)
{
    if (ptr == NULL || len == 0) return nil;
    return [[NSString alloc] initWithBytes:ptr length:len encoding:NSUTF8StringEncoding];
}

static PHAsset *MacAssetForIdentifier(const uint8_t *identifier_ptr, size_t identifier_len)
{
    NSString *identifier = MacStringFromBytes(identifier_ptr, identifier_len);
    if (identifier.length == 0) return nil;
    PHFetchResult<PHAsset *> *result = [PHAsset fetchAssetsWithLocalIdentifiers:@[ identifier ] options:nil];
    return result.firstObject;
}

static CGSize MacMediaTargetSize(PHAsset *asset, uint8_t size)
{
    CGFloat edge = 512.0;
    if (size == 0) edge = 160.0;
    else if (size == 2) edge = 1024.0;
    CGFloat width = MAX((CGFloat)asset.pixelWidth, 1.0);
    CGFloat height = MAX((CGFloat)asset.pixelHeight, 1.0);
    CGFloat scale = MIN(edge / width, edge / height);
    scale = MIN(MAX(scale, 0.01), 1.0);
    return CGSizeMake(ceil(width * scale), ceil(height * scale));
}

static NSImage *MacLoadImage(PHAsset *asset, BOOL thumbnail, uint8_t size)
{
    if (asset == nil || asset.mediaType != PHAssetMediaTypeImage) return nil;
    PHImageRequestOptions *options = [[PHImageRequestOptions alloc] init];
    options.synchronous = YES;
    options.networkAccessAllowed = YES;
    options.resizeMode = thumbnail ? PHImageRequestOptionsResizeModeFast
                                   : PHImageRequestOptionsResizeModeNone;
    options.deliveryMode = thumbnail ? PHImageRequestOptionsDeliveryModeFastFormat
                                     : PHImageRequestOptionsDeliveryModeHighQualityFormat;

    CGSize target = thumbnail
                        ? MacMediaTargetSize(asset, size)
                        : CGSizeMake(MAX((CGFloat)asset.pixelWidth, 1.0),
                                     MAX((CGFloat)asset.pixelHeight, 1.0));
    __block NSImage *image = nil;
    [[PHImageManager defaultManager]
        requestImageForAsset:asset
                  targetSize:target
                 contentMode:PHImageContentModeAspectFit
                     options:options
               resultHandler:^(NSImage *result, NSDictionary *info) {
                   (void)info;
                   image = result;
               }];
    return image;
}

static NSData *MacJpegDataFromImage(NSImage *image)
{
    if (image == nil) return nil;
    CGImageRef cg = [image CGImageForProposedRect:NULL context:nil hints:nil];
    if (cg == NULL) return nil;
    NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithCGImage:cg];
    return [rep representationUsingType:NSBitmapImageFileTypeJPEG
                             properties:@{ NSImageCompressionFactor : @0.92 }];
}

static NSData *MacBgraDataFromImage(NSImage *image, uint32_t *out_width, uint32_t *out_height, size_t *out_row_bytes)
{
    if (out_width != NULL) *out_width = 0;
    if (out_height != NULL) *out_height = 0;
    if (out_row_bytes != NULL) *out_row_bytes = 0;
    if (image == nil) return nil;
    CGImageRef cg = [image CGImageForProposedRect:NULL context:nil hints:nil];
    if (cg == NULL) return nil;

    size_t width = CGImageGetWidth(cg);
    size_t height = CGImageGetHeight(cg);
    if (width == 0 || height == 0) return nil;
    size_t row_bytes = width * 4;
    NSMutableData *data = [NSMutableData dataWithLength:row_bytes * height];
    CGColorSpaceRef color_space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef context = CGBitmapContextCreate(
        data.mutableBytes,
        width,
        height,
        8,
        row_bytes,
        color_space,
        kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst);
    CGColorSpaceRelease(color_space);
    if (context == NULL) return nil;

    CGContextDrawImage(context, CGRectMake(0, 0, width, height), cg);
    CGContextRelease(context);
    if (out_width != NULL) *out_width = (uint32_t)MIN(width, UINT32_MAX);
    if (out_height != NULL) *out_height = (uint32_t)MIN(height, UINT32_MAX);
    if (out_row_bytes != NULL) *out_row_bytes = row_bytes;
    return data;
}

static int32_t MacLoadImageData(const uint8_t *identifier_ptr,
                                size_t identifier_len,
                                BOOL thumbnail,
                                BOOL bgra,
                                uint8_t size,
                                OxideImageData *out_image)
{
    if (out_image == NULL) return OxideMediaErrInvalid;
    MacZeroImageData(out_image);
    if (!MacMediaLibraryAuthorized()) return OxideMediaErrPermissionDenied;
    PHAsset *asset = MacAssetForIdentifier(identifier_ptr, identifier_len);
    if (asset == nil || asset.mediaType != PHAssetMediaTypeImage) return OxideMediaNotFound;
    NSImage *image = MacLoadImage(asset, thumbnail, size);
    if (image == nil) return OxideMediaNotFound;

    NSData *data = nil;
    uint32_t width = (uint32_t)MIN((NSUInteger)MAX(asset.pixelWidth, 0), (NSUInteger)UINT32_MAX);
    uint32_t height = (uint32_t)MIN((NSUInteger)MAX(asset.pixelHeight, 0), (NSUInteger)UINT32_MAX);
    size_t row_bytes = 0;
    if (bgra) {
        data = MacBgraDataFromImage(image, &width, &height, &row_bytes);
    } else {
        data = MacJpegDataFromImage(image);
    }
    if (data == nil || data.length == 0) return OxideMediaNotFound;
    if (!MacCopyData(data, &out_image->data_ptr, &out_image->data_len)) return OxideMediaErrIo;
    out_image->width = width;
    out_image->height = height;
    out_image->row_bytes = row_bytes;
    return 0;
}

int32_t oxide_media_fetch_assets(uint8_t media_type_mask,
                                 int32_t limit,
                                 uint8_t ascending,
                                 const OxideMediaAsset **out_assets,
                                 size_t *out_count)
{
    if (out_assets != NULL) *out_assets = NULL;
    if (out_count != NULL) *out_count = 0;
    if (out_assets == NULL || out_count == NULL || limit < 0) return OxideMediaErrInvalid;
    if (!MacMediaLibraryAuthorized()) return OxideMediaErrPermissionDenied;

    PHFetchOptions *options = [[PHFetchOptions alloc] init];
    options.sortDescriptors =
        @[ [NSSortDescriptor sortDescriptorWithKey:@"creationDate" ascending:(ascending != 0)] ];
    if (limit > 0) {
        options.fetchLimit = (NSUInteger)limit;
    }
    if (media_type_mask == 1) {
        options.predicate = [NSPredicate predicateWithFormat:@"mediaType == %d", PHAssetMediaTypeImage];
    } else if (media_type_mask == 2) {
        options.predicate = [NSPredicate predicateWithFormat:@"mediaType == %d", PHAssetMediaTypeVideo];
    } else if (media_type_mask == 3) {
        options.predicate = [NSPredicate predicateWithFormat:@"mediaType == %d OR mediaType == %d",
                             PHAssetMediaTypeImage,
                             PHAssetMediaTypeVideo];
    } else {
        return 0;
    }

    PHFetchResult<PHAsset *> *result = [PHAsset fetchAssetsWithOptions:options];
    NSUInteger count = result.count;
    if (count == 0) return 0;

    OxideMediaAsset *assets = calloc(count, sizeof(OxideMediaAsset));
    if (assets == NULL) return OxideMediaErrIo;
    __block NSUInteger written = 0;
    __block BOOL copy_failed = NO;
    [result enumerateObjectsUsingBlock:^(PHAsset *asset, NSUInteger idx, BOOL *stop) {
        (void)idx;
        OxideMediaAsset *out = &assets[written];
        if (!MacCopyString(asset.localIdentifier, &out->identifier_ptr, &out->identifier_len)) {
            copy_failed = YES;
            *stop = YES;
            return;
        }
        out->media_type = asset.mediaType == PHAssetMediaTypeVideo ? 1 : 0;
        out->creation_date =
            asset.creationDate == nil ? 0 : (uint64_t)(asset.creationDate.timeIntervalSince1970 * 1000.0);
        out->duration_sec = asset.mediaType == PHAssetMediaTypeVideo ? asset.duration : 0.0;
        out->width = (uint32_t)MIN((NSUInteger)MAX(asset.pixelWidth, 0), (NSUInteger)UINT32_MAX);
        out->height = (uint32_t)MIN((NSUInteger)MAX(asset.pixelHeight, 0), (NSUInteger)UINT32_MAX);
        out->file_size = 0;
        written += 1;
    }];

    if (copy_failed) {
        for (NSUInteger i = 0; i < written; i += 1) {
            free((void *)assets[i].identifier_ptr);
        }
        free(assets);
        return OxideMediaErrIo;
    }

    if (written == 0) {
        free(assets);
        return 0;
    }
    *out_assets = assets;
    *out_count = written;
    return 0;
}

void oxide_media_free_assets(const OxideMediaAsset *assets, size_t count)
{
    if (assets == NULL) return;
    for (size_t i = 0; i < count; i += 1) {
        free((void *)assets[i].identifier_ptr);
    }
    free((void *)assets);
}

int32_t oxide_media_load_thumbnail(const uint8_t *identifier_ptr,
                                   size_t identifier_len,
                                   uint8_t size,
                                   OxideImageData *out_image)
{
    return MacLoadImageData(identifier_ptr, identifier_len, YES, NO, size, out_image);
}

int32_t oxide_media_load_thumbnail_rgba(const uint8_t *identifier_ptr,
                                        size_t identifier_len,
                                        uint8_t size,
                                        OxideImageData *out_image)
{
    return MacLoadImageData(identifier_ptr, identifier_len, YES, YES, size, out_image);
}

int32_t oxide_media_load_full_image(const uint8_t *identifier_ptr,
                                    size_t identifier_len,
                                    OxideImageData *out_image)
{
    return MacLoadImageData(identifier_ptr, identifier_len, NO, NO, 0, out_image);
}

int32_t oxide_media_load_full_image_rgba(const uint8_t *identifier_ptr,
                                         size_t identifier_len,
                                         OxideImageData *out_image)
{
    return MacLoadImageData(identifier_ptr, identifier_len, NO, YES, 0, out_image);
}

void oxide_media_free_image_data(const uint8_t *data_ptr, size_t data_len)
{
    (void)data_len;
    if (data_ptr != NULL) free((void *)data_ptr);
}

int32_t oxide_media_load_video_file(const uint8_t *identifier_ptr,
                                    size_t identifier_len,
                                    const uint8_t **out_path_ptr,
                                    size_t *out_path_len)
{
    if (out_path_ptr != NULL) *out_path_ptr = NULL;
    if (out_path_len != NULL) *out_path_len = 0;
    if (out_path_ptr == NULL || out_path_len == NULL) return OxideMediaErrInvalid;
    if (!MacMediaLibraryAuthorized()) return OxideMediaErrPermissionDenied;
    PHAsset *asset = MacAssetForIdentifier(identifier_ptr, identifier_len);
    if (asset == nil || asset.mediaType != PHAssetMediaTypeVideo) return OxideMediaNotFound;

    PHVideoRequestOptions *options = [[PHVideoRequestOptions alloc] init];
    options.networkAccessAllowed = YES;
    options.deliveryMode = PHVideoRequestOptionsDeliveryModeHighQualityFormat;
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    __block AVAsset *video_asset = nil;
    [[PHImageManager defaultManager]
        requestAVAssetForVideo:asset
                       options:options
                 resultHandler:^(AVAsset *avAsset, AVAudioMix *audioMix, NSDictionary *info) {
                     (void)audioMix;
                     (void)info;
                     video_asset = avAsset;
                     dispatch_semaphore_signal(semaphore);
    }];
    dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC);
    if (dispatch_semaphore_wait(semaphore, timeout) != 0) return OxideMediaErrIo;
    if (video_asset == nil) return OxideMediaNotFound;

    NSString *path = nil;
    if ([video_asset isKindOfClass:[AVURLAsset class]]) {
        path = ((AVURLAsset *)video_asset).URL.path;
    } else {
        AVAssetExportSession *export_session =
            [AVAssetExportSession exportSessionWithAsset:video_asset
                                              presetName:AVAssetExportPresetPassthrough];
        if (export_session == nil) return OxideMediaErrUnsupported;
        NSString *file_name = [NSString stringWithFormat:@"oxide-media-%@.mov", NSUUID.UUID.UUIDString];
        NSURL *output_url =
            [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:file_name]];
        export_session.outputURL = output_url;
        export_session.outputFileType = AVFileTypeQuickTimeMovie;
        dispatch_semaphore_t export_semaphore = dispatch_semaphore_create(0);
        [export_session exportAsynchronouslyWithCompletionHandler:^{
            dispatch_semaphore_signal(export_semaphore);
        }];
        if (dispatch_semaphore_wait(export_semaphore, timeout) != 0) {
            [export_session cancelExport];
            return OxideMediaErrIo;
        }
        if (export_session.status != AVAssetExportSessionStatusCompleted) return OxideMediaErrIo;
        path = output_url.path;
    }

    if (path.length == 0) return OxideMediaNotFound;
    return MacCopyString(path, out_path_ptr, out_path_len) ? 0 : OxideMediaErrIo;
}

void oxide_media_free_string(const uint8_t *data_ptr, size_t data_len)
{
    (void)data_len;
    if (data_ptr != NULL) free((void *)data_ptr);
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

void oxide_host_string_free(uint8_t *p) { if (p) free(p); }

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
