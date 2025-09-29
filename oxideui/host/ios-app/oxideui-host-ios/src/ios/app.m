#include <stddef.h>
#include <stdlib.h>
#include <limits.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <time.h>
#include <dispatch/dispatch.h>
#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#import <CoreVideo/CoreVideo.h>
#import <Metal/Metal.h>
#import <UserNotifications/UserNotifications.h>
#import <CoreLocation/CoreLocation.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <Photos/Photos.h>
#import <Contacts/Contacts.h>
#import <CoreBluetooth/CoreBluetooth.h>
#import <CoreBluetooth/CBPeripheral.h>
#import <CoreBluetooth/CBCharacteristic.h>
#import <CoreBluetooth/CBUUID.h>
#import <CoreMotion/CoreMotion.h>
#import <Network/Network.h>
#import <AudioToolbox/AudioToolbox.h>
#import <os/log.h>
#import <os/lock.h>

// Forward declaration for in-file logging overlay function
static void UILog(NSString *line);

static inline void OxLogImpl(NSString *msg)
{
   // NSLog to device log
   NSLog(@"[OxideUI] %@", msg);
   // os_log for devices that suppress NSLog in UI tests
   if (@available(iOS 12.0, *))
   {
      os_log_with_type(OS_LOG_DEFAULT, OS_LOG_TYPE_DEFAULT, "[OxideUI] %{public}@", msg);
   }
   // Mirror to on-screen UILog overlay if present
   UILog(msg);
}

#ifndef OXLOG
#define OXLOG(fmt, ...) OxLogImpl([NSString stringWithFormat:(fmt), ##__VA_ARGS__])
#endif

static void (*gReachabilityCallback)(uint32_t status, uint32_t iface, uint8_t expensive) = NULL;
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 120000
static nw_path_monitor_t gReachabilityMonitor = NULL;
static dispatch_queue_t gReachabilityQueue = NULL;
#endif

// ===== Rust FFI declarations (Objective-C -> Rust) =====
void oxideui_host_emit_window_resized(float w, float h, float scale, float safe_l, float safe_t, float safe_r, float safe_b);
void oxideui_host_on_memory_warning(void);
void oxideui_host_emit_text_commit(const char *utf8, size_t len);
void oxideui_host_emit_text_composition(uint32_t start, uint32_t end, const char *utf8, size_t len);
void oxideui_host_emit_text_selection(uint32_t start, uint32_t end);
void oxideui_host_emit_ime_shown(float x, float y, float w, float h);
void oxideui_host_emit_ime_hidden(void);
void oxideui_host_emit_touch(uint64_t id, uint32_t phase, float x, float y, float pressure, uint8_t has_pressure, float tilt_alt, float tilt_azi, uint8_t has_tilt, uint32_t device, uint64_t ts_ns);
void oxideui_host_emit_pointer(float x, float y, float dx, float dy, uint32_t buttons, uint32_t modifiers, uint64_t ts_ns);
void oxideui_host_emit_key(uint32_t code, const char *chars, size_t chars_len, uint8_t repeat, uint32_t modifiers, uint64_t ts_ns);
void oxideui_host_emit_pinch(float cx, float cy, float delta);
void oxideui_host_emit_double_tap(void);
void oxideui_host_emit_perm(uint32_t domain, uint32_t status);
void oxideui_host_emit_push_token(uint32_t provider, const char *utf8, size_t len);
void oxideui_host_emit_push_notify(const char *utf8, size_t len);

int32_t oxideui_host_app_init(uint32_t w, uint32_t h, float scale);
int32_t oxideui_host_app_frame(uint32_t w, uint32_t h, float scale);
int32_t oxideui_host_app_present(void *drawable_ptr, void *texture_ptr);
int32_t oxideui_host_app_stats(void *stats_out);
uint32_t oxideui_host_scene_count(void);
uint32_t oxideui_host_scene_name(uint32_t idx, char *out_ptr, uint32_t out_len);
uint32_t oxideui_host_current_scene(void);
int32_t oxideui_host_set_scene(uint32_t idx);
uint8_t oxideui_host_is_overlay_visible(void);
int32_t oxideui_host_set_overlay_visible(uint8_t on);
uint8_t oxideui_host_is_reduce_motion(void);
int32_t oxideui_host_set_reduce_motion(uint8_t on);
void oxideui_host_app_did_enter_background(void);
void oxideui_host_app_will_enter_foreground(void);
void oxideui_host_app_will_terminate(void);
void oxideui_host_on_memory_warning(void);

// ===== Rust FFI declarations (Rust -> Objective-C) =====
void oxideui_host_clipboard_set(const char *utf8, size_t len);
int  oxideui_host_clipboard_get(char **out_ptr, size_t *out_len);
void oxideui_host_string_free(char *p);
void oxideui_host_haptics_play(uint32_t pattern);
uint32_t oxideui_host_perm_status(uint32_t domain);
void oxideui_host_perm_request(uint32_t domain);
void oxideui_host_push_register(void);
int  oxideui_host_push_get_device_token(char **out_ptr, size_t *out_len);
void oxideui_host_push_set_badge(int32_t count);
void oxideui_host_push_clear_badge(void);
void oxideui_host_input_log(const char *utf8, size_t len);
int32_t oxideui_host_set_camera_running(uint8_t running);
uint8_t oxideui_ble_is_supported(void);
void oxideui_ble_init(void);
uint8_t oxideui_ble_powered_on(void);
void oxideui_ble_shutdown(void);
void oxideui_ble_start_scan(const uint16_t *uuids16, size_t count);
void oxideui_ble_stop_scan(void);
void oxideui_ble_connect(const uint8_t *addr, size_t addr_len);
void oxideui_ble_disconnect(const uint8_t *addr, size_t addr_len);
int  oxideui_ble_read_char(const uint8_t *addr, size_t addr_len, const uint16_t *uuid16);
int  oxideui_ble_write_char(const uint8_t *addr, size_t addr_len, const uint16_t *uuid16, const uint8_t *data, size_t len);
int  oxideui_ble_subscribe(const uint8_t *addr, size_t addr_len, const uint16_t *uuid16, uint8_t on);
int32_t oxideui_host_resource_read(const char *name, void **out_ptr, size_t *out_len);
void oxideui_host_set_resource_loader(uint8_t (*cb)(const char *, void **, size_t *));
void oxideui_host_ime_show(void);
void oxideui_host_ime_hide(void);
void oxideui_host_release_drawable(void *drawable_ptr);
int32_t oxideui_host_start(void);

int32_t oxideui_cam_start_default(void);
void oxideui_cam_stop(void);
int32_t oxideui_cam_set_fps(int32_t fps);
int32_t oxideui_cam_set_resolution_height(int32_t h);
int32_t oxideui_cam_set_bit_depth(int32_t bits);
int32_t oxideui_cam_set_color_space(int32_t id);
int32_t oxideui_cam_set_position(int32_t pos);
int32_t oxideui_cam_set_mode(int32_t mode);
void oxideui_host_set_camera_callback(void (*cb)(const OxCameraFrame *));
void oxideui_host_set_camera_audio_callback(void (*cb)(const OxCameraAudio *));
int32_t oxideui_cam_get_latest(void **y_tex, void **uv_tex, int32_t *w, int32_t *h);
int32_t oxideui_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w, int32_t *h,
                                 int32_t *bitdepth, int32_t *matrix,
                                 int32_t *video_range, int32_t *colorspace);
int32_t oxideui_host_power_lowpower(void);
int32_t oxideui_host_thermal_state(void);
int32_t oxideui_host_set_camera_options(uint8_t blur, float sigma, uint8_t grayscale, uint8_t animate);
int32_t oxideui_host_set_anim_play(uint8_t play);
int32_t oxideui_host_set_anim_progress(float normalized);
int32_t oxideui_host_set_damage_options(uint8_t enabled, float use_thresh, float prefilter);
int32_t oxideui_host_set_nine_slice(float slice_px, float alpha);
int32_t oxideui_host_set_sdf_font(float font_px);
int32_t oxideui_host_take_snapshot(void);
uint32_t oxideui_host_get_snapshot_status(char *out_ptr, uint32_t out_len);

// ===== Host state =====

typedef struct oxideui_host_stats_t
{
   float fps;
   uint32_t draws;
   uint32_t anims;
   uint32_t memory_warnings;
   float damage_pct;
   uint32_t damage_rects;
   float cam_coverage_pct;
   float cam_blur_ms;
   uint32_t cam_blur_updates;
   uint32_t cam_update_period_ms;
   uint8_t cam_paused;
   uint8_t cam_low_power;
   uint8_t cam_thermal;
   uint32_t cam_width;
   uint32_t cam_height;
   uint8_t cam_bit_depth;
   uint8_t cam_matrix;
   uint8_t cam_video_range;
   uint8_t cam_color_space;
   uint8_t cam_running;
   float cam_fps;
} oxideui_host_stats_t;

@class MetalView;
static __weak UIView *gMetalView = nil;
static NSMutableSet<id<CAMetalDrawable>> *gInFlightDrawables = nil;
static BOOL gHostAppReady = NO;
static id<MTLDevice> gMetalDevice = nil;
static UILabel *gUILogLabel = nil;

static void StartMetalCaptureIfEnabled(id<MTLDevice> dev)
{
   const char *env = getenv("OXIDEUI_CAPTURE_METAL");
   if (!env) { return; }
   if (!(strcmp(env, "1") == 0 || strcasecmp(env, "true") == 0)) { return; }
   if (@available(iOS 13.0, *))
   {
      MTLCaptureManager *mgr = [MTLCaptureManager sharedCaptureManager];
      if (mgr.isCapturing) { return; }
      MTLCaptureDescriptor *desc = [MTLCaptureDescriptor new];
      desc.captureObject = dev;
      desc.destination = MTLCaptureDestinationDeveloperTools;
      NSError *err = nil;
      [mgr startCaptureWithDescriptor:desc error:&err];
      if (err)
      {
         OXLOG(@"MTLCapture start error: %@", err.localizedDescription);
      }
      else
      {
         OXLOG(@"MTLCapture started (device=%p)", dev);
      }
   }
}

static void StopMetalCapture(void)
{
   if (@available(iOS 13.0, *))
   {
      MTLCaptureManager *mgr = [MTLCaptureManager sharedCaptureManager];
      if (mgr.isCapturing)
      {
         [mgr stopCapture];
         OXLOG(@"MTLCapture stopped");
      }
   }
}

static BOOL IsRunningUITest(void)
{
   static BOOL checked = NO;
   static BOOL cached = NO;
   if (!checked)
   {
      NSArray<NSString *> *arguments = NSProcessInfo.processInfo.arguments;
      NSDictionary<NSString *, NSString *> *env = NSProcessInfo.processInfo.environment;
      // Robust detection for XCTest / UI tests
      BOOL hintArg = [arguments containsObject:@"UITEST"] || [arguments containsObject:@"UITests"]; // project convention
      BOOL hintEnv = [[env objectForKey:@"UITEST"] boolValue];
      BOOL xcCfg = [env objectForKey:@"XCTestConfigurationFilePath"] != nil;
      BOOL xcInject = [env objectForKey:@"XCInjectBundleInto"] != nil; // unit tests
      BOOL isTesting = hintArg || hintEnv || xcCfg || xcInject;
      cached = isTesting;
      OXLOG(@"IsRunningUITest? %d (args=%@ envKeys=%@)", (int)cached, arguments, env.allKeys);
      checked = YES;
   }
   return cached;
}

static BOOL ShouldRender(void)
{
   // Allow opting into rendering during UI tests via env
   NSDictionary<NSString *, NSString *> *env = NSProcessInfo.processInfo.environment;
   NSString *v = [env objectForKey:@"OXIDEUI_RENDER_IN_TEST"];
   BOOL render_in_test = (v && ([v isEqualToString:@"1"] || [[v lowercaseString] isEqualToString:@"true"])) ? YES : NO;
   return render_in_test || !IsRunningUITest();
}

static NSString *BoolYesNo(BOOL value)
{
   return value ? @"yes" : @"no";
}

static NSString *CameraMatrixName(uint8_t code)
{
   switch (code)
   {
      case 1: return @"601";
      case 2: return @"2020";
      default: return @"709";
   }
}

static NSString *CameraRangeName(uint8_t code)
{
   return (code == 0) ? @"full" : @"video";
}

static inline uint64_t ts_now_ns(void)
{
   struct timespec ts;
   clock_gettime(CLOCK_MONOTONIC, &ts);
   return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static int DeviceMaxFPS(void)
{
   CADisplayLink *dl = [CADisplayLink displayLinkWithTarget:[NSNull null] selector:@selector(description)];
   int max = 60;
   if (@available(iOS 15.0, *)) { max = (int)dl.preferredFrameRateRange.maximum; }
   else if (@available(iOS 10.3, *)) { max = (int)dl.preferredFramesPerSecond; }
   [dl invalidate];
   return max > 0 ? max : 60;
}

static int CurrentTargetFPS(void)
{
   return oxideui_host_is_reduce_motion() ? 60 : DeviceMaxFPS();
}

static void dispatch_on_main(void (^block)(void))
{
   if ([NSThread isMainThread])
   {
      block();
   }
   else
   {
      dispatch_async(dispatch_get_main_queue(), block);
   }
}

static void dispatch_sync_on_main(void (^block)(void))
{
   if ([NSThread isMainThread])
   {
      block();
   }
   else
   {
      dispatch_sync(dispatch_get_main_queue(), block);
   }
}

static NSString *StringFromUtf8(const char *utf8, size_t len)
{
   if (!utf8 || len == 0)
   {
      return @"";
   }
   return [[NSString alloc] initWithBytes:utf8 length:len encoding:NSUTF8StringEncoding] ?: @"";
}

static UIWindow *ResolveWindow(UIView *view)
{
   if (view.window)
   {
      return view.window;
   }
   if (@available(iOS 13.0, *))
   {
      NSSet<UIScene *> *scenes = UIApplication.sharedApplication.connectedScenes;
      for (UIScene *scene in scenes)
      {
         if (![scene isKindOfClass:[UIWindowScene class]])
         {
            continue;
         }
         UIWindowScene *ws = (UIWindowScene *)scene;
         for (UIWindow *w in ws.windows)
         {
            if (w.isKeyWindow)
            {
               return w;
            }
         }
      }
      for (UIScene *scene in scenes)
      {
         if (![scene isKindOfClass:[UIWindowScene class]])
         {
            continue;
         }
         UIWindowScene *ws = (UIWindowScene *)scene;
         if (ws.windows.count > 0)
         {
            return ws.windows.firstObject;
         }
      }
   }
   id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
   if ([delegate respondsToSelector:@selector(window)])
   {
      UIWindow *dw = delegate.window;
      if (dw)
      {
         return dw;
      }
   }
   return nil;
}

static void EmitWindowMetricsForView(UIView *view)
{
   if (!view)
   {
      return;
   }
   UIWindow *window = ResolveWindow(view);
   if (!window)
   {
      return;
   }
   CGFloat scale = window.screen.nativeScale;
   CGSize size = view.bounds.size;
   UIEdgeInsets safe = UIEdgeInsetsZero;
   if (@available(iOS 11.0, *))
   {
      safe = window.safeAreaInsets;
   }
   OXLOG(@"EmitWindowMetricsForView size=(%.1f,%.1f) scale=%.2f safe=(%.1f,%.1f,%.1f,%.1f)",
         size.width, size.height, scale, safe.left, safe.top, safe.right, safe.bottom);
   oxideui_host_emit_window_resized((float)size.width, (float)size.height, (float)scale,
                                    (float)safe.left, (float)safe.top, (float)safe.right, (float)safe.bottom);
}

static void EnsureHostInitialized(UIView *view)
{
   if (IsRunningUITest() || gHostAppReady || !view)
   {
      OXLOG(@"EnsureHostInitialized early-exit (ui=%d ready=%d view=%@)", (int)IsRunningUITest(), (int)gHostAppReady, view);
      return;
   }
   UIWindow *window = ResolveWindow(view);
   if (!window)
   {
      OXLOG(@"EnsureHostInitialized: no window yet");
      return;
   }
   CAMetalLayer *layer = (CAMetalLayer *)view.layer;
   CGSize size = layer.drawableSize;
   CGFloat scale = window.screen.nativeScale;
   int32_t rc = oxideui_host_app_init((uint32_t)lrintf((float)size.width), (uint32_t)lrintf((float)size.height), (float)scale);
   if (rc == 0)
   {
      gHostAppReady = YES;
      EmitWindowMetricsForView(view);
      uint32_t count = oxideui_host_scene_count();
      OXLOG(@"host initialized width=%u height=%u scale=%g scenes=%u",
              (uint32_t)lrintf((float)size.width), (uint32_t)lrintf((float)size.height), (double)scale, count);
   }
   else
   {
      OXLOG(@"host init failed rc=%d", rc);
   }
}

// ===== Clipboard / utility exports =====

void oxideui_host_clipboard_set(const char *utf8, size_t len)
{
   NSString *s = StringFromUtf8(utf8, len);
   dispatch_on_main(^{ [UIPasteboard generalPasteboard].string = s; });
}

int oxideui_host_clipboard_get(char **out_ptr, size_t *out_len)
{
   if (!out_ptr || !out_len)
   {
      return 0;
   }
   __block NSString *value = nil;
   dispatch_on_main(^{ value = [UIPasteboard generalPasteboard].string ?: @""; });
   if (!value)
   {
      *out_ptr = NULL;
      *out_len = 0;
      return 0;
   }
   NSData *data = [value dataUsingEncoding:NSUTF8StringEncoding];
   if (!data)
   {
      *out_ptr = NULL;
      *out_len = 0;
      return 0;
   }
   void *buf = malloc(data.length);
   if (!buf)
   {
      *out_ptr = NULL;
      *out_len = 0;
      return 0;
   }
   memcpy(buf, data.bytes, data.length);
   *out_ptr = buf;
   *out_len = data.length;
   return 1;
}

void oxideui_host_string_free(char *p)
{
   if (p)
   {
      free(p);
   }
}

void oxideui_host_haptics_play(uint32_t pattern)
{
   dispatch_on_main(^{
      if (pattern <= 2)
      {
         UIImpactFeedbackStyle style = UIImpactFeedbackStyleLight;
         if (pattern == 1) style = UIImpactFeedbackStyleMedium;
         else if (pattern == 2) style = UIImpactFeedbackStyleHeavy;
         UIImpactFeedbackGenerator *gen = [[UIImpactFeedbackGenerator alloc] initWithStyle:style];
         [gen impactOccurred];
      }
      else if (pattern == 3)
      {
         UISelectionFeedbackGenerator *sel = [UISelectionFeedbackGenerator new];
         [sel selectionChanged];
      }
      else
      {
         UINotificationFeedbackGenerator *note = [UINotificationFeedbackGenerator new];
         UINotificationFeedbackType t = UINotificationFeedbackTypeSuccess;
         if (pattern == 5) t = UINotificationFeedbackTypeWarning;
         else if (pattern == 6) t = UINotificationFeedbackTypeError;
         [note notificationOccurred:t];
      }
   });
}

// ===== Permissions =====

enum
{
   kOxPermStatusNotDetermined = 0,
   kOxPermStatusDenied = 1,
   kOxPermStatusLimited = 2,
   kOxPermStatusAuthorized = 3,
};

enum
{
   kOxPermDomainNotifications = 0,
   kOxPermDomainLocation = 1,
   kOxPermDomainCamera = 2,
   kOxPermDomainContacts = 3,
   kOxPermDomainBluetooth = 4,
   kOxPermDomainMotion = 5,
   kOxPermDomainMicrophone = 6,
   kOxPermDomainMediaLibrary = 7,
};

typedef struct
{
   double latitude;
   double longitude;
   double altitude;
   double horizontal_accuracy;
   double vertical_accuracy;
   double speed;
   double course;
   uint64_t timestamp_ms;
} OxLocationSample;

typedef struct
{
   uint32_t accuracy_kind;
   double distance_filter_m;
   uint8_t allow_background;
   uint8_t precise;
} OxLocationConfig;

typedef struct
{
    double pressure_pa;
    double relative_altitude_m;
    uint64_t timestamp_ms;
    uint8_t has_pressure;
    uint8_t has_relative_altitude;
} OxMotionSample;

typedef struct
{
    const uint8_t *services16;
    size_t service_count;
    uint8_t allow_duplicates;
} OxBleScanConfig;

typedef struct
{
    uint8_t id16[16];
    const uint8_t *name_utf8;
    size_t name_len;
    int16_t rssi_dbm;
    const uint8_t *services16;
    size_t service_count;
    const uint8_t *manufacturer_data;
    size_t manufacturer_len;
    uint8_t connectable;
} OxBleScanInfo;

typedef struct
{
    const uint8_t *y_plane;
    size_t y_len;
    size_t y_stride;
    const uint8_t *uv_plane;
    size_t uv_len;
    size_t uv_stride;
    int32_t width;
    int32_t height;
    uint64_t timestamp_ns;
    uint16_t rotation_deg;
    uint8_t bit_depth;
    uint8_t matrix;
    uint8_t video_range;
} OxCameraFrame;

typedef struct
{
   const int16_t *audio_ptr;
   size_t sample_count;
   uint32_t channels;
   uint32_t sample_rate_hz;
   uint64_t timestamp_ns;
} OxCameraAudio;

typedef struct
{
   uint32_t kind;
   const uint8_t *path_ptr;
   size_t path_len;
   uint64_t duration_ns;
   uint64_t size_bytes;
   uint8_t had_audio;
   int32_t error_code;
   const uint8_t *error_msg_ptr;
   size_t error_msg_len;
} OxCameraRecordEvent;

enum
{
   kOxCamRecordEventCompleted = 0,
   kOxCamRecordEventCancelled = 1,
   kOxCamRecordEventFailed = 2
};

enum
{
   kOxCamRecordErrorUnknown = 0,
   kOxCamRecordErrorPermission = 1,
   kOxCamRecordErrorCapability = 2,
   kOxCamRecordErrorNotFound = 3,
   kOxCamRecordErrorBusy = 4,
   kOxCamRecordErrorInvalid = 5,
   kOxCamRecordErrorUnsupported = 6,
   kOxCamRecordErrorIo = 7
};

static OxLocationSample gLastLocationSample;
static BOOL gHasLastLocation = NO;
static void (*gLocationCallback)(const OxLocationSample *) = NULL;
static void (*gLocationErrorCallback)(const uint8_t *, size_t) = NULL;

static CMAltimeter *gMotionAltimeter = nil;
static NSOperationQueue *gMotionQueue = nil;
static void (*gMotionCallback)(const OxMotionSample *) = NULL;
static OxMotionSample gLastMotionSample;
static BOOL gHasLastMotion = NO;
static BOOL gMotionActive = NO;
static void (*gCameraFrameCallback)(const OxCameraFrame *) = NULL;
static void (*gCameraAudioCallback)(const OxCameraAudio *) = NULL;
static void (*gCameraRecordCallback)(const OxCameraRecordEvent *) = NULL;
static NSString * const kOxCameraRecordErrorDomain = @"com.oxideui.camera.record";

static uint64_t timestamp_ms_now(void)
{
   struct timespec ts;
   clock_gettime(CLOCK_REALTIME, &ts);
   return (uint64_t)ts.tv_sec * 1000ull + (uint64_t)(ts.tv_nsec / 1000000ull);
}

static uint64_t CMTimeToNs(CMTime t)
{
   if (CMTIME_IS_INVALID(t) || t.timescale == 0)
   {
      return 0;
   }
   CMTime ns = CMTimeConvertScale(t, 1000000000, kCMTimeRoundingMethod_Default);
   if (ns.timescale == 0 || ns.value < 0)
   {
      return 0;
   }
   return (uint64_t)ns.value;
}

static uint64_t CMTimeSpanToNs(CMTime start, CMTime end)
{
   if (CMTIME_IS_INVALID(start) || CMTIME_IS_INVALID(end))
   {
      return 0;
   }
   CMTime span = CMTimeSubtract(end, start);
   if (span.timescale == 0 || span.value <= 0)
   {
      return 0;
   }
   return CMTimeToNs(span);
}

static void EmitCameraRecordEvent(uint32_t kind, NSURL *url, uint64_t duration_ns, uint64_t size_bytes, BOOL had_audio, int32_t error_code, NSString *error_msg)
{
   if (!gCameraRecordCallback)
   {
      return;
   }
   NSData *pathData = nil;
   if (url.path.length)
   {
      pathData = [url.path dataUsingEncoding:NSUTF8StringEncoding];
   }
   const uint8_t *path_ptr = (const uint8_t *)pathData.bytes;
   size_t path_len = pathData.length;

   NSData *msgData = nil;
   if (error_msg.length)
   {
      msgData = [error_msg dataUsingEncoding:NSUTF8StringEncoding];
   }
   const uint8_t *msg_ptr = (const uint8_t *)msgData.bytes;
   size_t msg_len = msgData.length;

   OxCameraRecordEvent ev = {
      .kind = kind,
      .path_ptr = path_ptr,
      .path_len = path_len,
      .duration_ns = duration_ns,
      .size_bytes = size_bytes,
      .had_audio = had_audio ? 1 : 0,
      .error_code = error_code,
      .error_msg_ptr = msg_ptr,
      .error_msg_len = msg_len
   };
   gCameraRecordCallback(&ev);
}

static OxLocationSample OxLocationSampleFrom(CLLocation *loc)
{
   OxLocationSample sample;
   sample.latitude = loc.coordinate.latitude;
   sample.longitude = loc.coordinate.longitude;
   sample.altitude = loc.altitude;
   sample.horizontal_accuracy = loc.horizontalAccuracy;
   sample.vertical_accuracy = loc.verticalAccuracy;
   sample.speed = loc.speed;
   sample.course = loc.course;
   sample.timestamp_ms = (uint64_t)(loc.timestamp.timeIntervalSince1970 * 1000.0);
   return sample;
}

static void DeliverLocationSample(OxLocationSample sample)
{
   gLastLocationSample = sample;
   gHasLastLocation = YES;
   if (gLocationCallback)
   {
      gLocationCallback(&gLastLocationSample);
   }
}

static void DeliverLocationError(NSError *error)
{
   if (!gLocationErrorCallback)
   {
      return;
   }
   NSString *msg = error.localizedDescription ?: @"unknown location error";
   NSData *utf8 = [msg dataUsingEncoding:NSUTF8StringEncoding];
   gLocationErrorCallback((const uint8_t *)utf8.bytes, utf8.length);
}

static void DeliverMotionSample(OxMotionSample sample)
{
   gLastMotionSample = sample;
   gHasLastMotion = YES;
   if (gMotionCallback)
   {
      gMotionCallback(&gLastMotionSample);
   }
}

void oxideui_host_set_camera_callback(void (*cb)(const OxCameraFrame *))
{
   gCameraFrameCallback = cb;
}

void oxideui_host_set_camera_audio_callback(void (*cb)(const OxCameraAudio *))
{
   gCameraAudioCallback = cb;
}

void oxideui_host_set_camera_record_callback(void (*cb)(const OxCameraRecordEvent *))
{
   gCameraRecordCallback = cb;
}

static void UUIDBytesFromNSUUID(NSUUID *uuid, uint8_t out[16])
{
   uuid_t bytes;
   [uuid getUUIDBytes:bytes];
   memcpy(out, bytes, 16);
}

static void FillCBUUIDBytes(CBUUID *uuid, uint8_t out[16])
{
   memset(out, 0, 16);
   NSData *data = uuid.data;
   if (!data) return;
   const uint8_t *bytes = data.bytes;
   if (!bytes) return;
   if (data.length >= 16)
   {
      memcpy(out, bytes, 16);
   }
   else if (data.length == 4)
   {
      memcpy(out, bytes, 4);
   }
   else if (data.length == 2)
   {
      // Embed 16-bit UUID into Bluetooth base UUID (little-endian)
      static const uint8_t base[16] = { 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
                                        0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB };
      memcpy(out, base, 16);
      out[0] = bytes[0];
      out[1] = bytes[1];
   }
   else
   {
      memcpy(out, bytes, data.length);
   }
}

static NSArray<CBUUID *> *BleServicesFromConfig(const OxBleScanConfig *cfg)
{
   if (!cfg || cfg->service_count == 0 || cfg->services16 == NULL)
   {
      return nil;
   }
   NSMutableArray<CBUUID *> *services = [NSMutableArray arrayWithCapacity:cfg->service_count];
   for (size_t i = 0; i < cfg->service_count; ++i)
   {
      const uint8_t *ptr = cfg->services16 + (i * 16);
      NSData *data = [NSData dataWithBytes:ptr length:16];
      CBUUID *uuid = [CBUUID UUIDWithData:data];
      if (uuid) [services addObject:uuid];
   }
   return services.count ? services : nil;
}

@interface OxBleCentral : NSObject <CBCentralManagerDelegate, CBPeripheralDelegate>
@property (nonatomic, strong) CBCentralManager *central;
@property (nonatomic, strong) NSMutableDictionary<NSUUID *, CBPeripheral *> *peripherals;
@end

static OxBleCentral *BleContext(void)
{
   static OxBleCentral *ctx = nil;
   static dispatch_once_t onceToken;
   dispatch_once(&onceToken, ^{
      ctx = [OxBleCentral new];
   });
   return ctx;
}

@implementation OxBleCentral

- (instancetype)init
{
   self = [super init];
   if (self)
   {
      _peripherals = [NSMutableDictionary dictionary];
      _central = [[CBCentralManager alloc] initWithDelegate:self queue:dispatch_get_main_queue() options:@{ CBCentralManagerOptionShowPowerAlertKey : @NO }];
   }
   return self;
}

- (void)centralManagerDidUpdateState:(CBCentralManager *)central
{
   oxideui_host_ble_emit_state((central.state == CBManagerStatePoweredOn) ? 1 : 0);
   EmitPermissionAsync(kOxPermDomainBluetooth, StatusFromBluetoothAuthorization());
}

- (void)centralManager:(CBCentralManager *)central
   didDiscoverPeripheral:(CBPeripheral *)peripheral
       advertisementData:(NSDictionary<NSString *, id> *)advertisementData
                    RSSI:(NSNumber *)RSSI
{
   if (!peripheral.identifier)
   {
      return;
   }

   self.peripherals[peripheral.identifier] = peripheral;

   uint8_t id_bytes[16];
   UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);

   NSString *name = peripheral.name.length ? peripheral.name : nil;
   if (!name)
   {
      name = advertisementData[CBAdvertisementDataLocalNameKey];
   }
   NSData *nameData = name.length ? [name dataUsingEncoding:NSUTF8StringEncoding] : nil;

   NSArray<CBUUID *> *serviceUUIDs = advertisementData[CBAdvertisementDataServiceUUIDsKey];
   NSMutableData *serviceData = nil;
   if (serviceUUIDs.count)
   {
      serviceData = [NSMutableData dataWithLength:serviceUUIDs.count * 16];
      uint8_t *dst = serviceData.mutableBytes;
      NSUInteger idx = 0;
      for (CBUUID *uuid in serviceUUIDs)
      {
         uint8_t buf[16];
         FillCBUUIDBytes(uuid, buf);
         memcpy(dst + idx * 16, buf, 16);
         idx += 1;
      }
   }

   NSData *manufacturer = advertisementData[CBAdvertisementDataManufacturerDataKey];
   NSNumber *connectable = advertisementData[CBAdvertisementDataIsConnectable];

   OxBleScanInfo info = {0};
   memcpy(info.id16, id_bytes, 16);
   if (nameData)
   {
      info.name_utf8 = (const uint8_t *)nameData.bytes;
      info.name_len = nameData.length;
   }
   info.rssi_dbm = RSSI ? RSSI.shortValue : 0;
   if (serviceData)
   {
      info.services16 = (const uint8_t *)serviceData.bytes;
      info.service_count = serviceUUIDs.count;
   }
   if (manufacturer)
   {
      info.manufacturer_data = (const uint8_t *)manufacturer.bytes;
      info.manufacturer_len = manufacturer.length;
   }
   info.connectable = connectable.boolValue ? 1 : 0;

   oxideui_host_ble_emit_discovered(&info);
}

- (void)centralManager:(CBCentralManager *)central didConnectPeripheral:(CBPeripheral *)peripheral
{
   if (peripheral.identifier)
   {
      self.peripherals[peripheral.identifier] = peripheral;
   }
   peripheral.delegate = self;
   uint8_t id_bytes[16];
   UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
   oxideui_host_ble_emit_connected(id_bytes);
}

- (void)centralManager:(CBCentralManager *)central
    didFailToConnectPeripheral:(CBPeripheral *)peripheral
                         error:(NSError *)error
{
   (void)central;
   (void)error;
   uint8_t id_bytes[16];
   UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
   oxideui_host_ble_emit_disconnected(id_bytes);
}

- (void)centralManager:(CBCentralManager *)central
 didDisconnectPeripheral:(CBPeripheral *)peripheral
                 error:(NSError *)error
{
   (void)central;
   (void)error;
   uint8_t id_bytes[16];
   UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
   oxideui_host_ble_emit_disconnected(id_bytes);
}

@end


void oxideui_host_set_location_callback(void (*cb)(const OxLocationSample *))
{
   gLocationCallback = cb;
}

void oxideui_host_set_location_error_callback(void (*cb)(const uint8_t *, size_t))
{
   gLocationErrorCallback = cb;
}

static CLLocationAccuracy AccuracyForConfig(OxLocationConfig cfg)
{
   switch (cfg.accuracy_kind)
   {
      case 0: return kCLLocationAccuracyKilometer;
      case 2: return kCLLocationAccuracyBest;
      default: return kCLLocationAccuracyNearestTenMeters;
   }
}

int32_t oxideui_host_location_start(OxLocationConfig cfg)
{
   __block int32_t rc = 0;
   dispatch_on_main(^{
      CLLocationManager *mgr = CurrentLocationManager();
      if (!mgr)
      {
         rc = -1;
         return;
      }
      mgr.desiredAccuracy = AccuracyForConfig(cfg);
      mgr.distanceFilter = (cfg.distance_filter_m > 0.0) ? cfg.distance_filter_m : kCLDistanceFilterNone;
      if ([mgr respondsToSelector:@selector(setAllowsBackgroundLocationUpdates:)])
      {
         mgr.allowsBackgroundLocationUpdates = (cfg.allow_background != 0);
      }
      if ([mgr respondsToSelector:@selector(setShowsBackgroundLocationIndicator:)])
      {
         mgr.showsBackgroundLocationIndicator = (cfg.allow_background != 0);
      }
      mgr.pausesLocationUpdatesAutomatically = (cfg.allow_background == 0);
      if (cfg.precise && cfg.accuracy_kind != 2)
      {
         mgr.desiredAccuracy = kCLLocationAccuracyBest;
      }
      gHasLastLocation = NO;
      [mgr startUpdatingLocation];
   });
   return rc;
}

void oxideui_host_location_stop(void)
{
   dispatch_on_main(^{
      CLLocationManager *mgr = CurrentLocationManager();
      [mgr stopUpdatingLocation];
   });
}

void oxideui_host_location_request_once(void)
{
   dispatch_on_main(^{
      CLLocationManager *mgr = CurrentLocationManager();
      if ([mgr respondsToSelector:@selector(requestLocation)])
      {
         [mgr requestLocation];
      }
   });
}

uint8_t oxideui_host_location_last(OxLocationSample *out_sample)
{
   __block uint8_t has = 0;
   dispatch_sync(dispatch_get_main_queue(), ^{
      if (gHasLastLocation && out_sample)
      {
         *out_sample = gLastLocationSample;
         has = 1;
      }
   });
   return has;
}

void oxideui_host_set_motion_callback(void (*cb)(const OxMotionSample *))
{
   gMotionCallback = cb;
}

int32_t oxideui_host_motion_start(void)
{
   if (![CMAltimeter isRelativeAltitudeAvailable])
   {
      gMotionActive = NO;
      return -1;
   }
   dispatch_on_main(^{
      if (!gMotionAltimeter)
      {
         gMotionAltimeter = [CMAltimeter new];
      }
      if (!gMotionQueue)
      {
         gMotionQueue = [NSOperationQueue mainQueue];
      }
      gHasLastMotion = NO;
      gMotionActive = YES;
      [gMotionAltimeter startRelativeAltitudeUpdatesToQueue:gMotionQueue withHandler:^(CMAltitudeData * _Nullable altitudeData, NSError * _Nullable error) {
         if (error)
         {
            gMotionActive = NO;
            return;
         }
         if (!altitudeData)
         {
            return;
         }
         OxMotionSample sample;
         sample.pressure_pa = altitudeData.pressure.doubleValue * 1000.0;
         sample.has_pressure = 1;
         sample.relative_altitude_m = altitudeData.relativeAltitude.doubleValue;
         sample.has_relative_altitude = 1;
         sample.timestamp_ms = timestamp_ms_now();
         DeliverMotionSample(sample);
      }];
   });
   return 0;
}

void oxideui_host_motion_stop(void)
{
   dispatch_on_main(^{
      if (gMotionAltimeter)
      {
         [gMotionAltimeter stopRelativeAltitudeUpdates];
      }
      gMotionActive = NO;
   });
}

uint8_t oxideui_host_motion_is_active(void)
{
   return gMotionActive ? 1 : 0;
}

static void EmitPermissionAsync(uint32_t domain, uint32_t status)
{
   dispatch_async(dispatch_get_main_queue(), ^{
      oxideui_host_emit_perm(domain, status);
   });
}

static uint32_t StatusFromAVAuthorization(AVAuthorizationStatus status)
{
   switch (status)
   {
      case AVAuthorizationStatusAuthorized: return kOxPermStatusAuthorized;
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
      case AVAuthorizationStatusLimited: return kOxPermStatusLimited;
#endif
      case AVAuthorizationStatusDenied:
      case AVAuthorizationStatusRestricted: return kOxPermStatusDenied;
      case AVAuthorizationStatusNotDetermined:
      default: return kOxPermStatusNotDetermined;
   }
}

static uint32_t StatusFromRecordPermission(AVAudioSessionRecordPermission perm)
{
   switch (perm)
   {
      case AVAudioSessionRecordPermissionGranted: return kOxPermStatusAuthorized;
      case AVAudioSessionRecordPermissionDenied: return kOxPermStatusDenied;
      case AVAudioSessionRecordPermissionUndetermined:
      default: return kOxPermStatusNotDetermined;
   }
}

static uint32_t StatusFromPhotoAuthorization(PHAuthorizationStatus status)
{
   switch (status)
   {
      case PHAuthorizationStatusAuthorized: return kOxPermStatusAuthorized;
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
      case PHAuthorizationStatusLimited: return kOxPermStatusLimited;
#endif
      case PHAuthorizationStatusDenied:
      case PHAuthorizationStatusRestricted: return kOxPermStatusDenied;
      case PHAuthorizationStatusNotDetermined:
      default: return kOxPermStatusNotDetermined;
   }
}

static uint32_t StatusFromContactAuthorization(CNAuthorizationStatus status)
{
   switch (status)
   {
      case CNAuthorizationStatusAuthorized: return kOxPermStatusAuthorized;
      case CNAuthorizationStatusDenied:
      case CNAuthorizationStatusRestricted: return kOxPermStatusDenied;
      case CNAuthorizationStatusNotDetermined:
      default: return kOxPermStatusNotDetermined;
   }
}

static uint32_t StatusFromNotificationSettings(UNNotificationSettings *settings)
{
   switch (settings.authorizationStatus)
   {
      case UNAuthorizationStatusAuthorized:
      {
         if (settings.alertSetting == UNNotificationSettingEnabled ||
             settings.badgeSetting == UNNotificationSettingEnabled ||
             settings.soundSetting == UNNotificationSettingEnabled)
         {
            return kOxPermStatusAuthorized;
         }
         return kOxPermStatusLimited;
      }
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 120000
      case UNAuthorizationStatusProvisional:
#endif
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 130000
      case UNAuthorizationStatusEphemeral:
#endif
         return kOxPermStatusLimited;
      case UNAuthorizationStatusDenied:
         return kOxPermStatusDenied;
      case UNAuthorizationStatusNotDetermined:
      default:
         return kOxPermStatusNotDetermined;
   }
}

static uint32_t CurrentNotificationStatus(void)
{
   __block uint32_t status = kOxPermStatusNotDetermined;
   dispatch_semaphore_t sem = dispatch_semaphore_create(0);
   void (^query)(void) = ^{
      [[UNUserNotificationCenter currentNotificationCenter] getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings) {
         status = StatusFromNotificationSettings(settings);
         dispatch_semaphore_signal(sem);
      }];
   };
   if ([NSThread isMainThread])
   {
      dispatch_async(dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), query);
   }
   else
   {
      query();
   }
   int64_t timeout = (int64_t)(0.5 * NSEC_PER_SEC);
   dispatch_time_t deadline = dispatch_time(DISPATCH_TIME_NOW, timeout);
   if (dispatch_semaphore_wait(sem, deadline) != 0)
   {
      status = kOxPermStatusNotDetermined;
   }
   return status;
}

static uint32_t StatusFromBluetoothAuthorization(void)
{
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 130000
   if (@available(iOS 13.0, *))
   {
      switch ([CBManager authorization])
      {
         case CBManagerAuthorizationAllowedAlways: return kOxPermStatusAuthorized;
         case CBManagerAuthorizationDenied: return kOxPermStatusDenied;
         case CBManagerAuthorizationRestricted: return kOxPermStatusDenied;
         case CBManagerAuthorizationNotDetermined:
         default: return kOxPermStatusNotDetermined;
      }
   }
#endif
   return kOxPermStatusAuthorized;
}

static uint32_t StatusFromMotionAuthorization(void)
{
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 110000
   if (@available(iOS 11.0, *))
   {
      switch ([CMMotionActivityManager authorizationStatus])
      {
         case CMAuthorizationStatusAuthorized: return kOxPermStatusAuthorized;
         case CMAuthorizationStatusDenied: return kOxPermStatusDenied;
         case CMAuthorizationStatusRestricted: return kOxPermStatusDenied;
         case CMAuthorizationStatusNotDetermined:
         default: return kOxPermStatusNotDetermined;
      }
   }
#endif
   return kOxPermStatusAuthorized;
}

@interface OxPermLocationDelegate : NSObject <CLLocationManagerDelegate>
@end

@implementation OxPermLocationDelegate
- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager
{
   EmitPermissionAsync(kOxPermDomainLocation, oxideui_host_perm_status(kOxPermDomainLocation));
}
- (void)locationManager:(CLLocationManager *)manager didChangeAuthorizationStatus:(CLAuthorizationStatus)status
{
   (void)status;
   EmitPermissionAsync(kOxPermDomainLocation, oxideui_host_perm_status(kOxPermDomainLocation));
}
- (void)locationManager:(CLLocationManager *)manager didUpdateLocations:(NSArray<CLLocation *> *)locations
{
   (void)manager;
   CLLocation *loc = locations.lastObject;
   if (!loc)
   {
      return;
   }
   DeliverLocationSample(OxLocationSampleFrom(loc));
}
- (void)locationManager:(CLLocationManager *)manager didFailWithError:(NSError *)error
{
   (void)manager;
   if (error)
   {
      DeliverLocationError(error);
   }
}
@end

static CLLocationManager *CurrentLocationManager(void)
{
   static CLLocationManager *manager = nil;
   static OxPermLocationDelegate *delegate = nil;
   static dispatch_once_t onceToken;
   dispatch_once(&onceToken, ^{
      delegate = [OxPermLocationDelegate new];
      manager = [CLLocationManager new];
      manager.delegate = delegate;
   });
   return manager;
}

static uint32_t StatusFromLocationAuthorization(void)
{
   CLLocationManager *manager = CurrentLocationManager();
   CLAuthorizationStatus status;
   if (@available(iOS 14.0, *))
   {
      status = manager.authorizationStatus;
   }
   else
   {
      status = [CLLocationManager authorizationStatus];
   }
   switch (status)
   {
      case kCLAuthorizationStatusAuthorizedAlways:
      case kCLAuthorizationStatusAuthorizedWhenInUse:
      case kCLAuthorizationStatusAuthorized:
      {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
         if (@available(iOS 14.0, *))
         {
            if (manager.accuracyAuthorization == CLAccuracyAuthorizationReducedAccuracy)
            {
               return kOxPermStatusLimited;
            }
         }
#endif
         return kOxPermStatusAuthorized;
      }
      case kCLAuthorizationStatusDenied:
      case kCLAuthorizationStatusRestricted:
         return kOxPermStatusDenied;
      case kCLAuthorizationStatusNotDetermined:
      default:
         return kOxPermStatusNotDetermined;
   }
}

static CBCentralManager *CurrentBluetoothManager(void)
{
   return BleContext().central;
}

uint32_t oxideui_host_perm_status(uint32_t domain)
{
   switch (domain)
   {
      case kOxPermDomainNotifications: return CurrentNotificationStatus();
      case kOxPermDomainLocation: return StatusFromLocationAuthorization();
      case kOxPermDomainCamera: return StatusFromAVAuthorization([AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo]);
      case kOxPermDomainContacts: return StatusFromContactAuthorization([CNContactStore authorizationStatusForEntityType:CNEntityTypeContacts]);
      case kOxPermDomainBluetooth:
         (void)CurrentBluetoothManager();
         return StatusFromBluetoothAuthorization();
      case kOxPermDomainMotion: return StatusFromMotionAuthorization();
      case kOxPermDomainMicrophone: return StatusFromRecordPermission([AVAudioSession sharedInstance].recordPermission);
      case kOxPermDomainMediaLibrary:
      {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
         if (@available(iOS 14.0, *))
         {
            return StatusFromPhotoAuthorization([PHPhotoLibrary authorizationStatusForAccessLevel:PHAccessLevelReadWrite]);
         }
#endif
         return StatusFromPhotoAuthorization([PHPhotoLibrary authorizationStatus]);
      }
      default: return kOxPermStatusDenied;
   }
}

void oxideui_host_perm_request(uint32_t domain)
{
   switch (domain)
   {
      case kOxPermDomainNotifications:
      {
         UNAuthorizationOptions opts = UNAuthorizationOptionAlert | UNAuthorizationOptionBadge | UNAuthorizationOptionSound;
         [[UNUserNotificationCenter currentNotificationCenter] requestAuthorizationWithOptions:opts completionHandler:^(BOOL granted, NSError *error) {
            (void)granted; (void)error;
            [[UNUserNotificationCenter currentNotificationCenter] getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings) {
               EmitPermissionAsync(kOxPermDomainNotifications, StatusFromNotificationSettings(settings));
            }];
         }];
         break;
      }
      case kOxPermDomainLocation:
      {
         dispatch_async(dispatch_get_main_queue(), ^{
            [CurrentLocationManager() requestWhenInUseAuthorization];
         });
         EmitPermissionAsync(kOxPermDomainLocation, StatusFromLocationAuthorization());
         break;
      }
      case kOxPermDomainCamera:
      {
         [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL granted) {
            (void)granted;
            EmitPermissionAsync(kOxPermDomainCamera, StatusFromAVAuthorization([AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo]));
         }];
         break;
      }
      case kOxPermDomainContacts:
      {
         CNContactStore *store = [CNContactStore new];
         [store requestAccessForEntityType:CNEntityTypeContacts completionHandler:^(BOOL granted, NSError *error) {
            (void)granted; (void)error;
            EmitPermissionAsync(kOxPermDomainContacts, StatusFromContactAuthorization([CNContactStore authorizationStatusForEntityType:CNEntityTypeContacts]));
         }];
         break;
      }
      case kOxPermDomainBluetooth:
      {
         (void)CurrentBluetoothManager();
         EmitPermissionAsync(kOxPermDomainBluetooth, StatusFromBluetoothAuthorization());
         break;
      }
      case kOxPermDomainMotion:
      {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 110000
         if (@available(iOS 11.0, *))
         {
            [CMMotionActivityManager requestAuthorization:^(CMAuthorizationStatus status) {
               (void)status;
               EmitPermissionAsync(kOxPermDomainMotion, StatusFromMotionAuthorization());
            }];
         }
         else
         {
            EmitPermissionAsync(kOxPermDomainMotion, StatusFromMotionAuthorization());
         }
#else
         EmitPermissionAsync(kOxPermDomainMotion, StatusFromMotionAuthorization());
#endif
         break;
      }
      case kOxPermDomainMicrophone:
      {
         [[AVAudioSession sharedInstance] requestRecordPermission:^(BOOL granted) {
            (void)granted;
            EmitPermissionAsync(kOxPermDomainMicrophone, StatusFromRecordPermission([AVAudioSession sharedInstance].recordPermission));
         }];
         break;
      }
      case kOxPermDomainMediaLibrary:
      {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
         if (@available(iOS 14.0, *))
         {
            [PHPhotoLibrary requestAuthorizationForAccessLevel:PHAccessLevelReadWrite handler:^(PHAuthorizationStatus status) {
               EmitPermissionAsync(kOxPermDomainMediaLibrary, StatusFromPhotoAuthorization(status));
            }];
            break;
         }
#endif
         [PHPhotoLibrary requestAuthorization:^(PHAuthorizationStatus status) {
            EmitPermissionAsync(kOxPermDomainMediaLibrary, StatusFromPhotoAuthorization(status));
         }];
         break;
      }
      default:
         EmitPermissionAsync(domain, kOxPermStatusDenied);
         break;
   }
}

// ===== Push stubs =====

void oxideui_host_push_register(void) {}

int oxideui_host_push_get_device_token(char **out_ptr, size_t *out_len)
{
   if (out_ptr) *out_ptr = NULL;
   if (out_len) *out_len = 0;
   return 0;
}

void oxideui_host_push_set_badge(int32_t count)
{
   dispatch_on_main(^{
      if (@available(iOS 17.0, *))
      {
         [[UNUserNotificationCenter currentNotificationCenter] setBadgeCount:count withCompletionHandler:nil];
      }
      else
      {
         UIApplication.sharedApplication.applicationIconBadgeNumber = count;
      }
   });
}

void oxideui_host_push_clear_badge(void)
{
   oxideui_host_push_set_badge(0);
}

// ===== Reachability bridge =====

static uint32_t ReachabilityInterfaceForPath(nw_path_t path)
{
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 120000
   __block uint32_t iface = 3;
   nw_path_enumerate_interfaces(path, ^bool(nw_interface_t interface) {
      nw_interface_type_t type = nw_interface_get_type(interface);
      switch (type)
      {
         case nw_interface_type_wifi:
            iface = 0;
            return false;
         case nw_interface_type_cellular:
            iface = 1;
            return false;
#if defined(nw_interface_type_wiredEthernet)
         case nw_interface_type_wiredEthernet:
            iface = 2;
            return false;
#endif
#if defined(nw_interface_type_wired)
         case nw_interface_type_wired:
            iface = 2;
            return false;
#endif
         default:
            break;
      }
      return true;
   });
   return iface;
#else
   (void)path;
   return 3;
#endif
}

static void ReachabilityEmit(nw_path_t path)
{
   if (!gReachabilityCallback)
   {
      return;
   }
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 120000
   if (!path)
   {
      gReachabilityCallback(0, 3, 0);
      return;
   }
   nw_path_status_t status = nw_path_get_status(path);
   if (status == nw_path_status_satisfied)
   {
      uint32_t iface = ReachabilityInterfaceForPath(path);
      uint8_t expensive = nw_path_is_expensive(path) ? 1 : 0;
      gReachabilityCallback(1, iface, expensive);
   }
   else
   {
      gReachabilityCallback(0, 3, 0);
   }
#else
   (void)path;
   gReachabilityCallback(0, 3, 0);
#endif
}

void oxideui_host_net_set_reachability_callback(void (*cb)(uint32_t status, uint32_t iface, uint8_t expensive))
{
   gReachabilityCallback = cb;
}

int32_t oxideui_host_net_start_reachability(void)
{
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 120000
   if (!@available(iOS 12.0, *))
   {
      return -1;
   }
   if (gReachabilityMonitor)
   {
      return 0;
   }
   gReachabilityMonitor = nw_path_monitor_create();
   if (!gReachabilityMonitor)
   {
      return -1;
   }
   gReachabilityQueue = dispatch_queue_create("com.oxideui.reachability", DISPATCH_QUEUE_SERIAL);
   nw_path_monitor_set_queue(gReachabilityMonitor, gReachabilityQueue);
   nw_path_monitor_set_update_handler(gReachabilityMonitor, ^(nw_path_t path) {
      ReachabilityEmit(path);
   });
   nw_path_monitor_start(gReachabilityMonitor);
   nw_path_t current = nw_path_monitor_copy_current_path(gReachabilityMonitor);
   if (current)
   {
      ReachabilityEmit(current);
      nw_release(current);
   }
   return 0;
#else
   (void)gReachabilityCallback;
   return -1;
#endif
}

void oxideui_host_net_stop_reachability(void)
{
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 120000
   if (@available(iOS 12.0, *))
   {
      if (gReachabilityMonitor)
      {
         nw_path_monitor_cancel(gReachabilityMonitor);
         nw_release(gReachabilityMonitor);
         gReachabilityMonitor = NULL;
      }
      gReachabilityQueue = NULL;
   }
#endif
}

// ===== BLE bridge =====

static NSUUID *NSUUIDFromBytes(const uint8_t *bytes, size_t len)
{
   if (!bytes || len < 16)
   {
      return nil;
   }
   uuid_t uuidBytes;
   memcpy(uuidBytes, bytes, 16);
   return [[NSUUID alloc] initWithUUIDBytes:uuidBytes];
}

uint8_t oxideui_ble_is_supported(void)
{
   return [CBCentralManager class] ? 1 : 0;
}

void oxideui_ble_init(void)
{
   dispatch_on_main(^{
      (void)BleContext();
   });
}

uint8_t oxideui_ble_powered_on(void)
{
   __block uint8_t on = 0;
   dispatch_sync(dispatch_get_main_queue(), ^{
      CBCentralManager *mgr = BleContext().central;
      on = (mgr.state == CBManagerStatePoweredOn) ? 1 : 0;
   });
   return on;
}

void oxideui_ble_shutdown(void)
{
   dispatch_on_main(^{
      OxBleCentral *ctx = BleContext();
      if (ctx.central.isScanning)
      {
         [ctx.central stopScan];
      }
      for (CBPeripheral *peripheral in ctx.peripherals.allValues)
      {
         if (peripheral.state == CBPeripheralStateConnected ||
             peripheral.state == CBPeripheralStateConnecting)
         {
            [ctx.central cancelPeripheralConnection:peripheral];
         }
      }
      [ctx.peripherals removeAllObjects];
   });
}

void oxideui_ble_start_scan(const OxBleScanConfig *cfg)
{
   dispatch_on_main(^{
      OxBleCentral *ctx = BleContext();
      if (!ctx.central || ctx.central.state != CBManagerStatePoweredOn)
      {
         return;
      }
      NSArray<CBUUID *> *services = BleServicesFromConfig(cfg);
      NSDictionary *options = @{ CBCentralManagerScanOptionAllowDuplicatesKey : @((cfg && cfg->allow_duplicates) ? YES : NO) };
      [ctx.central scanForPeripheralsWithServices:services options:options];
   });
}

void oxideui_ble_stop_scan(void)
{
   dispatch_on_main(^{
      OxBleCentral *ctx = BleContext();
      if (ctx.central.isScanning)
      {
         [ctx.central stopScan];
      }
   });
}

void oxideui_ble_connect(const uint8_t *addr, size_t addr_len)
{
   NSUUID *uuid = NSUUIDFromBytes(addr, addr_len);
   if (!uuid)
   {
      return;
   }
   dispatch_on_main(^{
      OxBleCentral *ctx = BleContext();
      CBPeripheral *peripheral = ctx.peripherals[uuid];
      if (!peripheral)
      {
         NSArray<CBPeripheral *> *retrieved = [ctx.central retrievePeripheralsWithIdentifiers:@[ uuid ]];
         peripheral = retrieved.firstObject;
         if (peripheral)
         {
            ctx.peripherals[uuid] = peripheral;
         }
      }
      if (peripheral)
      {
         [ctx.central connectPeripheral:peripheral options:nil];
      }
   });
}

void oxideui_ble_disconnect(const uint8_t *addr, size_t addr_len)
{
   NSUUID *uuid = NSUUIDFromBytes(addr, addr_len);
   if (!uuid)
   {
      return;
   }
   dispatch_on_main(^{
      OxBleCentral *ctx = BleContext();
      CBPeripheral *peripheral = ctx.peripherals[uuid];
      if (peripheral)
      {
         [ctx.central cancelPeripheralConnection:peripheral];
      }
   });
}

int oxideui_ble_read_char(const uint8_t *addr, size_t addr_len, const uint16_t *uuid16)
{
   (void)addr; (void)addr_len; (void)uuid16;
   return -1;
}

int oxideui_ble_write_char(const uint8_t *addr, size_t addr_len, const uint16_t *uuid16, const uint8_t *data, size_t len)
{
   (void)addr; (void)addr_len; (void)uuid16; (void)data; (void)len;
   return -1;
}

int oxideui_ble_subscribe(const uint8_t *addr, size_t addr_len, const uint16_t *uuid16, uint8_t on)
{
   (void)addr; (void)addr_len; (void)uuid16; (void)on;
   return -1;
}

// ===== Resource loading =====

static uint8_t (*gResourceLoader)(const char *, void **, size_t *) = NULL;

void oxideui_host_set_resource_loader(uint8_t (*cb)(const char *, void **, size_t *))
{
   gResourceLoader = cb;
}

int32_t oxideui_host_resource_read(const char *name, void **out_ptr, size_t *out_len)
{
   if (!out_ptr || !out_len || !name)
   {
      return 0;
   }
   if (gResourceLoader)
   {
      uint8_t ok = gResourceLoader(name, out_ptr, out_len);
      if (ok) { return 1; }
   }
   NSString *resource = StringFromUtf8(name, strlen(name));
   if (resource.length == 0)
   {
      *out_ptr = NULL;
      *out_len = 0;
      return 0;
   }
   NSString *bundlePath = [[NSBundle mainBundle] resourcePath];
   NSString *fullPath = [bundlePath stringByAppendingPathComponent:resource];
   NSData *data = [NSData dataWithContentsOfFile:fullPath options:NSDataReadingMappedIfSafe error:nil];
   if (!data)
   {
      *out_ptr = NULL;
      *out_len = 0;
      return 0;
   }
   void *buf = malloc(data.length);
   if (!buf)
   {
      *out_ptr = NULL;
      *out_len = 0;
      return 0;
   }
   memcpy(buf, data.bytes, data.length);
   *out_ptr = buf;
   *out_len = data.length;
   return 1;
}

// ===== IME control =====

void oxideui_host_ime_show(void)
{
   dispatch_on_main(^{
      UIView *view = gMetalView;
      if (view)
      {
         [view becomeFirstResponder];
      }
   });
}

void oxideui_host_ime_hide(void)
{
   dispatch_on_main(^{
      UIView *view = gMetalView;
      if (view)
      {
         [view resignFirstResponder];
      }
   });
}

void oxideui_host_release_drawable(void *drawable_ptr)
{
   if (!drawable_ptr)
   {
      return;
   }
   dispatch_async(dispatch_get_main_queue(), ^{
      // Optional delayed release for debugging lifetime races
      static dispatch_once_t once;
      static int delay_ms = 0;
      dispatch_once(&once, ^{
         NSString *env = [NSProcessInfo.processInfo.environment objectForKey:@"OXIDEUI_DELAY_RELEASE_MS"];
         if (env) { delay_ms = env.intValue; }
         OXLOG(@"release setup: delay_ms=%d", delay_ms);
      });
      // Log pointer identity for correlation with present logs
      OXLOG(@"releasing drawable %p (main=%d) delay=%dms", drawable_ptr, (int)[NSThread isMainThread], delay_ms);
      id<CAMetalDrawable> drawable = (__bridge id<CAMetalDrawable>)drawable_ptr;
      void (^releaseBlock)(void) = ^{
         if (gInFlightDrawables)
         {
            [gInFlightDrawables removeObject:drawable];
            OXLOG(@"inflight count(after release)=%lu", (unsigned long)gInFlightDrawables.count);
         }
      };
      if (delay_ms > 0)
      {
         dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)delay_ms * (int64_t)NSEC_PER_MSEC), dispatch_get_main_queue(), releaseBlock);
      }
      else
      {
         releaseBlock();
      }
   });
}

// ===== Camera implementation (AVFoundation + CVMetalTextureCache) =====

@interface CamCapture : NSObject<AVCaptureVideoDataOutputSampleBufferDelegate, AVCaptureAudioDataOutputSampleBufferDelegate>
@property(nonatomic, strong) AVCaptureSession *session;
@property(nonatomic, strong) AVCaptureDeviceInput *input;
@property(nonatomic, strong) AVCaptureVideoDataOutput *output;
@property(nonatomic, strong) AVCaptureDeviceInput *audioInput;
@property(nonatomic, strong) AVCaptureAudioDataOutput *audioOutput;
@property(nonatomic) dispatch_queue_t audioQueue;
@property(nonatomic) CVMetalTextureCacheRef cache;
@property(nonatomic) CVMetalTextureRef yRef;
@property(nonatomic) CVMetalTextureRef uvRef;
@property(nonatomic, strong) id<MTLTexture> yTex;
@property(nonatomic, strong) id<MTLTexture> uvTex;
@property(nonatomic) int width;
@property(nonatomic) int height;
@property(nonatomic) int bitDepth;   // 8 or 10
@property(nonatomic) int matrix;     // 0=709,1=601,2=2020
@property(nonatomic) int videoRange; // 0 full, 1 video
@property(nonatomic) int colorSpace; // reserved
@property(nonatomic) os_unfair_lock lock;
@property(nonatomic) BOOL running;
@property(nonatomic) int desiredFps;
@property(nonatomic) int desiredHeight;
@property(nonatomic) int desiredBitDepth; // 8 or 10
@property(nonatomic) int desiredColorSpace; // reserved
@property(nonatomic) AVCaptureDevicePosition desiredPosition;
@property(nonatomic) NSInteger desiredMode;
@property(nonatomic, strong) AVAssetWriter *writer;
@property(nonatomic, strong) AVAssetWriterInput *writerVideo;
@property(nonatomic, strong) AVAssetWriterInput *writerAudio;
@property(nonatomic, strong) NSURL *recordURL;
@property(nonatomic) BOOL recording;
@property(nonatomic) BOOL recordIncludeAudio;
@property(nonatomic) BOOL recordWriterStarted;
@property(nonatomic) BOOL recordTemporary;
@property(nonatomic) uint64_t recordBytes;
@property(nonatomic) uint64_t recordDurationNs;
@property(nonatomic) CMTime recordStartPTS;
@property(nonatomic) CMTime recordLastPTS;

- (BOOL)startRecordingWithPath:(NSString *)path container:(int32_t)container includeAudio:(BOOL)includeAudio error:(NSError **)outError;
- (void)stopRecordingSession;
- (void)cancelRecordingSession;
- (BOOL)isRecording;
@end

@interface CamCapture ()
- (void)resetRecordingState;
- (void)failRecordingWithError:(NSError *)error;
- (void)ensureRecordSessionStarted:(CMTime)pts;
- (void)appendVideoSample:(CMSampleBufferRef)sampleBuffer;
- (void)appendAudioSample:(CMSampleBufferRef)sampleBuffer;
@end

@implementation CamCapture
- (instancetype)init
{
   self = [super init];
   if (self)
   {
      _session = [AVCaptureSession new];
      _output = [AVCaptureVideoDataOutput new];
      _audioInput = nil;
      _audioOutput = nil;
      _audioQueue = NULL;
      _lock = OS_UNFAIR_LOCK_INIT;
      _desiredFps = 30; _desiredHeight = 1080; _desiredBitDepth = 8; _desiredColorSpace = 0;
      _desiredPosition = AVCaptureDevicePositionBack;
      _desiredMode = 0;
      _bitDepth = 8; _matrix = 0; _videoRange = 0; _colorSpace = 0; _width = 0; _height = 0;
      _writer = nil;
      _writerVideo = nil;
      _writerAudio = nil;
      _recordURL = nil;
      _recording = NO;
      _recordIncludeAudio = NO;
      _recordWriterStarted = NO;
      _recordTemporary = NO;
      _recordBytes = 0;
      _recordDurationNs = 0;
      _recordStartPTS = kCMTimeInvalid;
      _recordLastPTS = kCMTimeInvalid;
      if (gMetalDevice)
      {
         CVReturn cr = CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, gMetalDevice, NULL, &_cache);
         if (cr != kCVReturnSuccess)
         {
            _cache = NULL;
            OXLOG(@"CamCapture: CVMetalTextureCacheCreate failed %d", cr);
         }
      }
   }
   return self;
}

- (void)dealloc
{
   if (_yRef) { CFRelease(_yRef); _yRef = NULL; }
   if (_uvRef) { CFRelease(_uvRef); _uvRef = NULL; }
   if (_cache) { CFRelease(_cache); _cache = NULL; }
}

- (AVCaptureDevice *)selectDevice
{
   AVCaptureDevicePosition position = self.desiredPosition;
   AVCaptureDeviceDiscoverySession *discovery = [AVCaptureDeviceDiscoverySession
      discoverySessionWithDeviceTypes:@[AVCaptureDeviceTypeBuiltInWideAngleCamera]
      mediaType:AVMediaTypeVideo
      position:position];
   for (AVCaptureDevice *dev in discovery.devices)
   {
      if (dev.position == position) { return dev; }
   }
   return [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
}

- (AVCaptureDeviceFormat *)selectFormatForDevice:(AVCaptureDevice *)device
{
   if (!device) { return nil; }
   int targetHeight = self.desiredHeight > 0 ? self.desiredHeight : 1080;
   int targetBitDepth = self.desiredBitDepth;
   AVCaptureDeviceFormat *best = device.activeFormat;
   int bestDiff = INT_MAX;
   for (AVCaptureDeviceFormat *format in device.formats)
   {
      FourCharCode subtype = CMFormatDescriptionGetMediaSubType(format.formatDescription);
      BOOL is10Bit = (subtype == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange ||
                      subtype == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange);
      BOOL is8Bit = (subtype == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange ||
                     subtype == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
      if (targetBitDepth == 10 && !is10Bit) { continue; }
      if (targetBitDepth == 8 && !(is8Bit || is10Bit)) { continue; }

      CMVideoDimensions dims = CMVideoFormatDescriptionGetDimensions(format.formatDescription);
      if (dims.height <= 0) { continue; }
      int diff = (int)labs((long)dims.height - (long)targetHeight);
      if (diff < bestDiff)
      {
         bestDiff = diff;
         best = format;
      }
   }
   return best;
}

- (BOOL)configureSession
{
   if (!self.session) { return NO; }
   [self.session beginConfiguration];
   if (self.input) { [self.session removeInput:self.input]; self.input = nil; }
   if (self.audioInput) { [self.session removeInput:self.audioInput]; self.audioInput = nil; }
   if (self.output && [[self.session outputs] containsObject:self.output]) { [self.session removeOutput:self.output]; }
   if (self.audioOutput && [[self.session outputs] containsObject:self.audioOutput]) { [self.session removeOutput:self.audioOutput]; self.audioOutput = nil; }
   self.audioQueue = NULL;
   self.session.sessionPreset = AVCaptureSessionPresetHigh;

   AVCaptureDevice *device = [self selectDevice];
   if (!device) { [self.session commitConfiguration]; return NO; }

   NSError *err = nil;
   self.input = [AVCaptureDeviceInput deviceInputWithDevice:device error:&err];
   if (err || ![self.session canAddInput:self.input])
   {
      OXLOG(@"CamCapture: cannot add input: %@", err);
      [self.session commitConfiguration];
      return NO;
   }
   [self.session addInput:self.input];

   NSDictionary *settings = @{ (NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange) };
   self.output.videoSettings = settings;
   self.output.alwaysDiscardsLateVideoFrames = YES;
   dispatch_queue_t q = dispatch_queue_create("com.oxideui.cam", DISPATCH_QUEUE_SERIAL);
   [self.output setSampleBufferDelegate:self queue:q];
   if (![self.session canAddOutput:self.output])
   {
      OXLOG(@"CamCapture: cannot add output");
      [self.session commitConfiguration];
      return NO;
   }
   [self.session addOutput:self.output];

   AVCaptureDevice *audioDevice = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
   if (audioDevice)
   {
      NSError *audioErr = nil;
      self.audioInput = [AVCaptureDeviceInput deviceInputWithDevice:audioDevice error:&audioErr];
      if (!audioErr && self.audioInput && [self.session canAddInput:self.audioInput])
      {
         [self.session addInput:self.audioInput];
      }
      else
      {
         if (audioErr)
         {
            OXLOG(@"CamCapture: cannot add audio input: %@", audioErr);
         }
         self.audioInput = nil;
      }
      self.audioOutput = [AVCaptureAudioDataOutput new];
      self.audioOutput.audioSettings = @{
         AVFormatIDKey: @(kAudioFormatLinearPCM),
         AVSampleRateKey: @(48000),
         AVNumberOfChannelsKey: @(2),
         AVLinearPCMBitDepthKey: @(16),
         AVLinearPCMIsFloatKey: @NO,
         AVLinearPCMIsNonInterleaved: @NO
      };
      self.audioQueue = dispatch_queue_create("com.oxideui.cam.audio", DISPATCH_QUEUE_SERIAL);
      [self.audioOutput setSampleBufferDelegate:self queue:self.audioQueue];
      if ([self.session canAddOutput:self.audioOutput])
      {
         [self.session addOutput:self.audioOutput];
      }
      else
      {
         OXLOG(@"CamCapture: cannot add audio output");
         self.audioOutput = nil;
      }
   }

   // Preferred fps if possible
   if ([device lockForConfiguration:&err])
   {
      int fps = self.desiredFps > 0 ? self.desiredFps : 30;
      CMTime dur = CMTimeMake(1, fps);
      if ([device respondsToSelector:@selector(setActiveVideoMinFrameDuration:)])
      {
         device.activeVideoMinFrameDuration = dur;
         device.activeVideoMaxFrameDuration = dur;
      }
      [device unlockForConfiguration];
   }
   [self.session commitConfiguration];
   return YES;
}

- (void)start
{
   if (self.running) { return; }
   AVAuthorizationStatus st = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo];
   if (st == AVAuthorizationStatusNotDetermined)
   {
      [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL granted) {
         if (granted) { dispatch_async(dispatch_get_main_queue(), ^{ [self start]; }); }
      }];
      return;
   }
   if (st != AVAuthorizationStatusAuthorized)
   {
      OXLOG(@"CamCapture: camera not authorized (status=%ld)", (long)st);
      return;
   }
   AVAudioSession *audioSession = [AVAudioSession sharedInstance];
   if ([audioSession respondsToSelector:@selector(recordPermission)])
   {
      AVAudioSessionRecordPermission perm = audioSession.recordPermission;
      if (perm == AVAudioSessionRecordPermissionUndetermined)
      {
         [audioSession requestRecordPermission:^(BOOL granted) {
            if (granted) { dispatch_async(dispatch_get_main_queue(), ^{ [self start]; }); }
         }];
         return;
      }
   }
   NSError *audioErr = nil;
   [audioSession setCategory:AVAudioSessionCategoryPlayAndRecord
                 withOptions:(AVAudioSessionCategoryOptionDefaultToSpeaker | AVAudioSessionCategoryOptionMixWithOthers)
                       error:&audioErr];
   [audioSession setMode:AVAudioSessionModeVideoRecording error:&audioErr];
   [audioSession setActive:YES error:&audioErr];

   if (![self configureSession]) { return; }
   [self.session startRunning];
   self.running = YES;
}

- (void)stop
{
   if (!self.running) { return; }
   if (self.recording)
   {
      [self cancelRecordingSession];
   }
   [self.session stopRunning];
   self.running = NO;
   NSError *audioErr = nil;
   [[AVAudioSession sharedInstance] setActive:NO error:&audioErr];
}

- (BOOL)isRecording
{
   return self.recording || self.writer != nil;
}

- (BOOL)startRecordingWithPath:(NSString *)path container:(int32_t)container includeAudio:(BOOL)includeAudio error:(NSError **)outError
{
   if (self.recording || self.writer != nil)
   {
      if (outError)
      {
         *outError = [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                          code:kOxCamRecordErrorBusy
                                      userInfo:@{ NSLocalizedDescriptionKey: @"Recording already active" }];
      }
      return NO;
   }
   if (!gCameraRecordCallback)
   {
      if (outError)
      {
         *outError = [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                          code:kOxCamRecordErrorCapability
                                      userInfo:@{ NSLocalizedDescriptionKey: @"No camera recording listener registered" }];
      }
      return NO;
   }

   NSString *extension = (container == 0) ? @"mp4" : @"mov";
   NSString *targetPath = path;
   BOOL temp = NO;
   if (targetPath.length == 0)
   {
      NSString *fileName = [NSString stringWithFormat:@"oxideui_%@.%@", [[NSUUID UUID] UUIDString], extension];
      targetPath = [NSTemporaryDirectory() stringByAppendingPathComponent:fileName];
      temp = YES;
   }
   NSURL *url = [NSURL fileURLWithPath:targetPath];
   if (!url)
   {
      if (outError)
      {
         *outError = [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                          code=kOxCamRecordErrorInvalid
                                      userInfo:@{ NSLocalizedDescriptionKey: @"Invalid recording destination" }];
      }
      return NO;
   }
   [[NSFileManager defaultManager] removeItemAtURL:url error:nil];

   AVFileType fileType = (container == 0) ? AVFileTypeMPEG4 : AVFileTypeQuickTimeMovie;
   NSError *writerErr = nil;
   AVAssetWriter *writer = [AVAssetWriter assetWriterWithURL:url fileType:fileType error:&writerErr];
   if (!writer)
   {
      if (outError)
      {
         *outError = writerErr ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                                     code=kOxCamRecordErrorIo
                                                 userInfo:@{ NSLocalizedDescriptionKey: @"Failed to create asset writer" }];
      }
      return NO;
   }
   writer.shouldOptimizeForNetworkUse = YES;

   int width = (self.width > 0) ? self.width : 1280;
   int height = (self.height > 0) ? self.height : 720;
   int fps = (self.desiredFps > 0) ? self.desiredFps : 30;
   NSDictionary *compression = @{
      AVVideoAverageBitRateKey: @(width * height * 6),
      AVVideoExpectedSourceFrameRateKey: @(fps),
      AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel
   };
   NSDictionary *videoSettings = @{
      AVVideoCodecKey: AVVideoCodecTypeH264,
      AVVideoWidthKey: @(width),
      AVVideoHeightKey: @(height),
      AVVideoCompressionPropertiesKey: compression
   };
   AVAssetWriterInput *videoInput = [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeVideo outputSettings:videoSettings];
   videoInput.expectsMediaDataInRealTime = YES;
   if (![writer canAddInput:videoInput])
   {
      if (outError)
      {
         *outError = [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                          code=kOxCamRecordErrorUnsupported
                                      userInfo:@{ NSLocalizedDescriptionKey: @"Writer cannot accept video input" }];
      }
      return NO;
   }
   [writer addInput:videoInput];

   BOOL audioEnabled = includeAudio && self.audioOutput != nil;
   AVAssetWriterInput *audioInput = nil;
   if (audioEnabled)
   {
      NSDictionary *audioSettings = @{
         AVFormatIDKey: @(kAudioFormatMPEG4AAC),
         AVNumberOfChannelsKey: @(2),
         AVSampleRateKey: @(48000),
         AVEncoderBitRateKey: @(128000)
      };
      audioInput = [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeAudio outputSettings:audioSettings];
      audioInput.expectsMediaDataInRealTime = YES;
      if ([writer canAddInput:audioInput])
      {
         [writer addInput:audioInput];
      }
      else
      {
         audioEnabled = NO;
         audioInput = nil;
      }
   }

   if (![writer startWriting])
   {
      NSError *err = writer.error ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                                         code=kOxCamRecordErrorIo
                                                     userInfo:@{ NSLocalizedDescriptionKey: @"Unable to start writer" }];
      if (outError) { *outError = err; }
      return NO;
   }

   self.writer = writer;
   self.writerVideo = videoInput;
   self.writerAudio = audioInput;
   self.recordURL = url;
   self.recording = YES;
   self.recordIncludeAudio = audioEnabled;
   self.recordWriterStarted = NO;
   self.recordTemporary = temp;
   self.recordBytes = 0;
   self.recordDurationNs = 0;
   self.recordStartPTS = kCMTimeInvalid;
   self.recordLastPTS = kCMTimeInvalid;
   return YES;
}

- (void)stopRecordingSession
{
   if (!self.writer)
   {
      return;
   }
   if (!self.recording && self.writer.status != AVAssetWriterStatusWriting)
   {
      return;
   }
   self.recording = NO;
   if (self.writerVideo)
   {
      [self.writerVideo markAsFinished];
   }
   if (self.recordIncludeAudio && self.writerAudio)
   {
      [self.writerAudio markAsFinished];
   }
   AVAssetWriter *writer = self.writer;
   NSURL *url = self.recordURL;
   BOOL hadAudio = self.recordIncludeAudio;
   uint64_t bytes = self.recordBytes;
   uint64_t duration = self.recordDurationNs;
   BOOL temp = self.recordTemporary;
   [writer finishWritingWithCompletionHandler:^{
      AVAssetWriterStatus status = writer.status;
      NSError *error = writer.error;
      dispatch_async(dispatch_get_main_queue(), ^{
         if (status == AVAssetWriterStatusCompleted)
         {
            EmitCameraRecordEvent(kOxCamRecordEventCompleted, url, duration, bytes, hadAudio, 0, nil);
         }
         else
         {
            NSString *message = error.localizedDescription ?: @"Recording failed";
            EmitCameraRecordEvent(kOxCamRecordEventFailed, url, duration, bytes, hadAudio, kOxCamRecordErrorIo, message);
            if (temp && url)
            {
               [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
            }
         }
         [self resetRecordingState];
      });
   }];
}

- (void)cancelRecordingSession
{
   if (!self.writer)
   {
      return;
   }
   NSURL *url = self.recordURL;
   BOOL temp = self.recordTemporary;
   BOOL hadAudio = self.recordIncludeAudio;
   self.recording = NO;
   [self.writer cancelWriting];
   [self resetRecordingState];
   if (temp && url)
   {
      [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
   }
   EmitCameraRecordEvent(kOxCamRecordEventCancelled, url, 0, 0, hadAudio, 0, nil);
}

- (void)resetRecordingState
{
   self.writer = nil;
   self.writerVideo = nil;
   self.writerAudio = nil;
   self.recordURL = nil;
   self.recordIncludeAudio = NO;
   self.recordWriterStarted = NO;
   self.recordTemporary = NO;
   self.recordBytes = 0;
   self.recordDurationNs = 0;
   self.recordStartPTS = kCMTimeInvalid;
   self.recordLastPTS = kCMTimeInvalid;
}

- (void)failRecordingWithError:(NSError *)error
{
   if (!self.writer)
   {
      return;
   }
   NSURL *url = self.recordURL;
   BOOL temp = self.recordTemporary;
   BOOL hadAudio = self.recordIncludeAudio;
   [self.writer cancelWriting];
   [self resetRecordingState];
   if (temp && url)
   {
      [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
   }
   NSString *message = error.localizedDescription ?: @"Recording error";
   EmitCameraRecordEvent(kOxCamRecordEventFailed, url, 0, 0, hadAudio, kOxCamRecordErrorIo, message);
}

- (void)ensureRecordSessionStarted:(CMTime)pts
{
   if (self.recordWriterStarted)
   {
      return;
   }
   if (!self.writer)
   {
      return;
   }
   if (!CMTIME_IS_VALID(pts))
   {
      return;
   }
   [self.writer startSessionAtSourceTime:pts];
   self.recordWriterStarted = YES;
   self.recordStartPTS = pts;
   self.recordLastPTS = pts;
   self.recordDurationNs = 0;
}

- (void)appendVideoSample:(CMSampleBufferRef)sampleBuffer
{
   if (!self.recording || !self.writer || self.writerVideo == nil)
   {
      return;
   }
   if (self.writer.status == AVAssetWriterStatusFailed)
   {
      [self failRecordingWithError:self.writer.error ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain code:kOxCamRecordErrorIo userInfo:nil]];
      return;
   }
   CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
   if (!self.recordWriterStarted)
   {
      [self ensureRecordSessionStarted:pts];
   }
   if (!self.recordWriterStarted)
   {
      return;
   }
   if (!self.writerVideo.isReadyForMoreMediaData)
   {
      return;
   }
   if (![self.writerVideo appendSampleBuffer:sampleBuffer])
   {
      [self failRecordingWithError:self.writer.error ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain code=kOxCamRecordErrorIo userInfo:nil]];
      return;
   }
   size_t bytes = (size_t)CMSampleBufferGetTotalSampleSize(sampleBuffer);
   self.recordBytes += bytes;
   self.recordLastPTS = pts;
   uint64_t span = CMTimeSpanToNs(self.recordStartPTS, pts);
   if (span > self.recordDurationNs)
   {
      self.recordDurationNs = span;
   }
}

- (void)appendAudioSample:(CMSampleBufferRef)sampleBuffer
{
   if (!self.recording || !self.recordIncludeAudio || !self.writer || self.writerAudio == nil)
   {
      return;
   }
   if (self.writer.status == AVAssetWriterStatusFailed)
   {
      [self failRecordingWithError:self.writer.error ?: [NSError errorWithDomain=kOxCameraRecordErrorDomain code=kOxCamRecordErrorIo userInfo:nil]];
      return;
   }
   CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
   if (!self.recordWriterStarted)
   {
      [self ensureRecordSessionStarted:pts];
   }
   if (!self.recordWriterStarted)
   {
      return;
   }
   if (!self.writerAudio.isReadyForMoreMediaData)
   {
      return;
   }
   if (![self.writerAudio appendSampleBuffer:sampleBuffer])
   {
      [self failRecordingWithError:self.writer.error ?: [NSError errorWithDomain=kOxCameraRecordErrorDomain code=kOxCamRecordErrorIo userInfo:nil]];
      return;
   }
   size_t bytes = (size_t)CMSampleBufferGetTotalSampleSize(sampleBuffer);
   self.recordBytes += bytes;
   uint64_t span = CMTimeSpanToNs(self.recordStartPTS, pts);
   if (span > self.recordDurationNs)
   {
      self.recordDurationNs = span;
   }
}

- (void)captureOutput:(AVCaptureOutput *)output didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer fromConnection:(AVCaptureConnection *)connection
{
   (void)connection;
   if (self.audioOutput && output == self.audioOutput)
   {
      BOOL wantsCallback = (gCameraAudioCallback != NULL);
      BOOL wantsRecording = (self.recordIncludeAudio && (self.recording || self.writer != nil));
      if (!wantsCallback && !wantsRecording)
      {
         return;
      }
      CMFormatDescriptionRef fmt = CMSampleBufferGetFormatDescription(sampleBuffer);
      const AudioStreamBasicDescription *asbd = fmt ? CMAudioFormatDescriptionGetStreamBasicDescription(fmt) : NULL;
      if (!asbd) { return; }
      uint32_t channels = (uint32_t)asbd->mChannelsPerFrame;
      uint32_t sampleRate = (uint32_t)asbd->mSampleRate;
      if (channels == 0 || sampleRate == 0) { return; }
      CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sampleBuffer);
      if (!block) { return; }
      size_t length = CMBlockBufferGetDataLength(block);
      if (length == 0) { return; }
      int16_t *buffer = (int16_t *)malloc(length);
      if (!buffer) { return; }
      if (CMBlockBufferCopyDataBytes(block, 0, length, buffer) != kCMBlockBufferNoErr) { free(buffer); return; }
      uint64_t timestamp_ns = 0;
      CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
      if (pts.timescale != 0)
      {
         CMTime nsTime = CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default);
         if (nsTime.timescale != 0)
         {
            timestamp_ns = (uint64_t)nsTime.value;
         }
      }
      [self appendAudioSample:sampleBuffer];
      if (wantsCallback)
      {
         int16_t *buffer = (int16_t *)malloc(length);
         if (!buffer) { return; }
         if (CMBlockBufferCopyDataBytes(block, 0, length, buffer) != kCMBlockBufferNoErr)
         {
            free(buffer);
            return;
         }
         OxCameraAudio audio = {
            .audio_ptr = buffer,
            .sample_count = length / sizeof(int16_t),
            .channels = channels,
            .sample_rate_hz = sampleRate,
            .timestamp_ns = timestamp_ns,
         };
        gCameraAudioCallback(&audio);
        free(buffer);
      }
      return;
   }
   if (output != self.output) { return; }
   CVImageBufferRef pb = CMSampleBufferGetImageBuffer(sampleBuffer);
   if (!pb || !self.cache) { return; }
   size_t wY = CVPixelBufferGetWidthOfPlane(pb, 0);
   size_t hY = CVPixelBufferGetHeightOfPlane(pb, 0);
   OSType fmt = CVPixelBufferGetPixelFormatType(pb);
   [self appendVideoSample:sampleBuffer];
   int bd = (fmt == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange || fmt == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange) ? 10 : 8;
   int vr = (fmt == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange || fmt == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange) ? 1 : 0;
   int mx = 0; // default 709
   if (@available(iOS 11.0, *))
   {
      CFTypeRef val = CVBufferCopyAttachment(pb, kCVImageBufferYCbCrMatrixKey, NULL);
      if (val && CFGetTypeID(val) == CFStringGetTypeID())
      {
         CFStringRef m = (CFStringRef)val;
         if (CFEqual(m, kCVImageBufferYCbCrMatrix_ITU_R_601_4)) mx = 1;
         else if (CFEqual(m, kCVImageBufferYCbCrMatrix_ITU_R_2020)) mx = 2;
         else mx = 0;
      }
      if (val) { CFRelease(val); }
   }
   CVMetalTextureRef yref = NULL; CVMetalTextureRef uvref = NULL;
   MTLPixelFormat yfmt = (bd == 10) ? MTLPixelFormatR16Unorm : MTLPixelFormatR8Unorm;
   MTLPixelFormat uvfmt = (bd == 10) ? MTLPixelFormatRG16Unorm : MTLPixelFormatRG8Unorm;
   CVReturn r0 = CVMetalTextureCacheCreateTextureFromImage(kCFAllocatorDefault, self.cache, pb, NULL, yfmt, wY, hY, 0, &yref);
   size_t wUV = CVPixelBufferGetWidthOfPlane(pb, 1);
   size_t hUV = CVPixelBufferGetHeightOfPlane(pb, 1);
   CVReturn r1 = CVMetalTextureCacheCreateTextureFromImage(kCFAllocatorDefault, self.cache, pb, NULL, uvfmt, wUV, hUV, 1, &uvref);
   if (r0 != kCVReturnSuccess || r1 != kCVReturnSuccess)
   {
      if (yref) CFRelease(yref); if (uvref) CFRelease(uvref);
      return;
   }
   id<MTLTexture> yTex = CVMetalTextureGetTexture(yref);
   id<MTLTexture> uvTex = CVMetalTextureGetTexture(uvref);
   if (!yTex || !uvTex)
   {
      if (yref) CFRelease(yref); if (uvref) CFRelease(uvref);
      return;
   }
   uint64_t timestamp_ns = 0;
   CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
   if (pts.timescale != 0)
   {
      CMTime nsTime = CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default);
      if (nsTime.timescale != 0)
      {
         timestamp_ns = (uint64_t)nsTime.value;
      }
   }
   CVPixelBufferLockBaseAddress(pb, kCVPixelBufferLock_ReadOnly);
   if (gCameraFrameCallback)
   {
      const uint8_t *yPlane = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
      const uint8_t *uvPlane = CVPixelBufferGetBaseAddressOfPlane(pb, 1);
      size_t strideY = CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
      size_t strideUV = CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
      size_t lenY = strideY * hY;
      size_t lenUV = strideUV * hUV;
      if (yPlane && uvPlane)
      {
         OxCameraFrame frame = {
            .y_plane = yPlane,
            .y_len = lenY,
            .y_stride = strideY,
            .uv_plane = uvPlane,
            .uv_len = lenUV,
            .uv_stride = strideUV,
            .width = (int32_t)wY,
            .height = (int32_t)hY,
            .timestamp_ns = timestamp_ns,
            .rotation_deg = 0,
            .bit_depth = (uint8_t)bd,
            .matrix = (uint8_t)mx,
            .video_range = (uint8_t)vr,
         };
         gCameraFrameCallback(&frame);
      }
   }
   CVPixelBufferUnlockBaseAddress(pb, kCVPixelBufferLock_ReadOnly);

   os_unfair_lock_lock(&_lock);
   if (self.yRef) { CFRelease(self.yRef); self.yRef = NULL; }
   if (self.uvRef) { CFRelease(self.uvRef); self.uvRef = NULL; }
   self.yRef = yref; self.uvRef = uvref;
   self.yTex = yTex; self.uvTex = uvTex;
   self.width = (int)wY; self.height = (int)hY;
   self.bitDepth = bd; self.matrix = mx; self.videoRange = vr; self.colorSpace = 0;
   os_unfair_lock_unlock(&_lock);
}

// Thread-safe snapshot of latest textures/metadata
- (BOOL)latestY:(id<MTLTexture> __strong *)y
             uv:(id<MTLTexture> __strong *)uv
              w:(int *)w
              h:(int *)h
        bitDepth:(int *)bd
           matrix:(int *)mx
       videoRange:(int *)vr
       colorSpace:(int *)cs
{
   os_unfair_lock_lock(&_lock);
   id<MTLTexture> ly = self.yTex;
   id<MTLTexture> luv = self.uvTex;
   int lw = self.width, lh = self.height, lbd = self.bitDepth, lmx = self.matrix, lvr = self.videoRange, lcs = self.colorSpace;
   os_unfair_lock_unlock(&_lock);
   if (!ly || !luv || lw <= 0 || lh <= 0) { return NO; }
   if (y) *y = ly;
   if (uv) *uv = luv;
   if (w) *w = lw;
   if (h) *h = lh;
   if (bd) *bd = lbd;
   if (mx) *mx = lmx;
   if (vr) *vr = lvr;
   if (cs) *cs = lcs;
   return YES;
}

- (void)restartIfRunning
{
   if (!self.running) { return; }
   [self stop];
   [self start];
}
@end

static CamCapture *gCam = nil;
static CamCapture *EnsureCam(void)
{
   static dispatch_once_t once;
   dispatch_once(&once, ^{ gCam = [CamCapture new]; });
   return gCam;
}

int32_t oxideui_cam_start_default(void)
{
   __block int32_t rc = 0;
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      [c start];
      rc = c.running ? 0 : -1;
   });
   return rc;
}

void oxideui_cam_stop(void)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      [c stop];
   });
}

int32_t oxideui_cam_set_fps(int32_t fps)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      c.desiredFps = (int)MAX(fps, 1);
      [c restartIfRunning];
   });
   return 0;
}

int32_t oxideui_cam_set_resolution_height(int32_t h)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      c.desiredHeight = (int)MAX(h, 0);
      [c restartIfRunning];
   });
   return 0;
}

int32_t oxideui_cam_set_bit_depth(int32_t bits)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      c.desiredBitDepth = (int)((bits >= 10) ? 10 : 8);
      [c restartIfRunning];
   });
   return 0;
}

int32_t oxideui_cam_set_color_space(int32_t id)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      c.desiredColorSpace = (int)id;
      [c restartIfRunning];
   });
   return 0;
}

int32_t oxideui_cam_set_position(int32_t pos)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      c.desiredPosition = (pos != 0) ? AVCaptureDevicePositionFront : AVCaptureDevicePositionBack;
      [c restartIfRunning];
   });
   return 0;
}

int32_t oxideui_cam_set_mode(int32_t mode)
{
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      c.desiredMode = mode;
      switch (mode)
      {
         case 1: // Photo
            c.desiredBitDepth = 10;
            c.desiredColorSpace = 1;
            break;
         case 2: // Video
            c.desiredBitDepth = 8;
            c.desiredColorSpace = 0;
            break;
         default:
            c.desiredBitDepth = 8;
            c.desiredColorSpace = 0;
            break;
      }
      [c restartIfRunning];
   });
   return 0;
}

int32_t oxideui_cam_record_start(const uint8_t *path_ptr, size_t path_len, int32_t container, uint8_t include_audio)
{
   __block int32_t rc = 0;
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      NSString *path = nil;
      if (path_ptr && path_len > 0)
      {
         path = [[NSString alloc] initWithBytes:path_ptr length:path_len encoding:NSUTF8StringEncoding];
      }
      NSError *err = nil;
      BOOL ok = [c startRecordingWithPath:path container:container includeAudio:(include_audio != 0) error:&err];
      if (!ok)
      {
         rc = -1;
         if (err)
         {
            OXLOG(@"CamCapture: start recording failed: %@", err.localizedDescription);
         }
      }
   });
   return rc;
}

int32_t oxideui_cam_record_stop(void)
{
   __block int32_t rc = 0;
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      if (![c isRecording])
      {
         rc = -1;
         return;
      }
      [c stopRecordingSession];
   });
   return rc;
}

int32_t oxideui_cam_record_cancel(void)
{
   __block int32_t rc = 0;
   dispatch_sync_on_main(^{
      CamCapture *c = EnsureCam();
      if (![c isRecording])
      {
         if (!c.writer)
         {
            rc = -1;
            return;
         }
      }
      [c cancelRecordingSession];
   });
   return rc;
}

int32_t oxideui_cam_get_latest(void **y_tex, void **uv_tex, int32_t *w, int32_t *h)
{
   int32_t bd, mx, vr, cs;
   return oxideui_cam_get_latest_ex(y_tex, uv_tex, w, h, &bd, &mx, &vr, &cs);
}

int32_t oxideui_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w, int32_t *h,
                                 int32_t *bitdepth, int32_t *matrix,
                                 int32_t *video_range, int32_t *colorspace)
{
   CamCapture *c = EnsureCam();
   id<MTLTexture> y = nil; id<MTLTexture> uv = nil; int width = 0, height = 0, bd = 8, mx = 0, vr = 0, cs = 0;
   if (![c latestY:&y uv:&uv w:&width h:&height bitDepth:&bd matrix:&mx videoRange:&vr colorSpace:&cs])
   {
      return 0;
   }
   if (y_tex) *y_tex = (__bridge void *)y;
   if (uv_tex) *uv_tex = (__bridge void *)uv;
   if (w) *w = (int32_t)width;
   if (h) *h = (int32_t)height;
   if (bitdepth) *bitdepth = (int32_t)bd;
   if (matrix) *matrix = (int32_t)mx;
   if (video_range) *video_range = (int32_t)vr;
   if (colorspace) *colorspace = (int32_t)cs;
   return 1;
}

int32_t oxideui_host_power_lowpower(void)
{
   if (@available(iOS 9.0, *))
   {
      return [[NSProcessInfo processInfo] isLowPowerModeEnabled] ? 1 : 0;
   }
   return 0;
}

int32_t oxideui_host_thermal_state(void)
{
   if (@available(iOS 11.0, *))
   {
      NSProcessInfoThermalState st = [NSProcessInfo processInfo].thermalState;
      switch (st)
      {
         case NSProcessInfoThermalStateNominal: return 0;
         case NSProcessInfoThermalStateFair: return 1;
         case NSProcessInfoThermalStateSerious: return 2;
         case NSProcessInfoThermalStateCritical: return 3;
      }
   }
   return 0;
}

// ===== Metal view =====

@interface MetalView : UIView <UIKeyInput>
@property(nonatomic) CGPoint lastHover;
@property(nonatomic, strong) NSMutableDictionary<NSValue *, NSNumber *> *touchIds;
@end

@implementation MetalView
+ (Class)layerClass { return [CAMetalLayer class]; }

- (instancetype)initWithFrame:(CGRect)frame
{
   self = [super initWithFrame:frame];
   if (self)
   {
      OXLOG(@"MetalView init");
      self.touchIds = [NSMutableDictionary dictionary];
      self.multipleTouchEnabled = YES;
      self.backgroundColor = [UIColor whiteColor];
      self.accessibilityIdentifier = @"metalView";

      CAMetalLayer *layer = (CAMetalLayer *)self.layer;
      // Bind a device and align format with renderer (sRGB)
      id<MTLDevice> dev = MTLCreateSystemDefaultDevice();
      gMetalDevice = dev;
      layer.device = dev;
      layer.pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB;
      layer.framebufferOnly = NO;
      layer.presentsWithTransaction = NO;
      if (@available(iOS 13.0, *)) { layer.allowsNextDrawableTimeout = NO; }
      if (@available(iOS 11.2, *)) { layer.maximumDrawableCount = 3; }
      layer.contentsScale = [UIScreen mainScreen].nativeScale;
      OXLOG(@"Layer setup: device=%p format=%lu framebufferOnly=%d contentsScale=%.2f maxDrawable=%lu",
            layer.device, (unsigned long)layer.pixelFormat, (int)layer.framebufferOnly, layer.contentsScale, (unsigned long)layer.maximumDrawableCount);
      StartMetalCaptureIfEnabled(dev);

      UIPinchGestureRecognizer *pinch = [[UIPinchGestureRecognizer alloc] initWithTarget:self action:@selector(onPinch:)];
      pinch.delaysTouchesBegan = NO;
      [self addGestureRecognizer:pinch];

      UITapGestureRecognizer *doubleTap = [[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(onDoubleTap:)];
      doubleTap.numberOfTapsRequired = 2;
      [self addGestureRecognizer:doubleTap];

      if (@available(iOS 13.4, *))
      {
         UIHoverGestureRecognizer *hover = [[UIHoverGestureRecognizer alloc] initWithTarget:self action:@selector(onHover:)];
         [self addGestureRecognizer:hover];
      }
   }
   return self;
}

- (BOOL)canBecomeFirstResponder { return YES; }
- (BOOL)hasText { return NO; }
- (void)insertText:(NSString *)text
{
   NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
   if (data.length > 0)
   {
      oxideui_host_emit_text_commit(data.bytes, data.length);
   }
}
- (void)deleteBackward
{
   const char backspace = '\b';
   oxideui_host_emit_text_commit(&backspace, 1);
}

- (void)layoutSubviews
{
   [super layoutSubviews];
   CAMetalLayer *layer = (CAMetalLayer *)self.layer;
   CGFloat scale = layer.contentsScale;
   layer.drawableSize = CGSizeMake(self.bounds.size.width * scale, self.bounds.size.height * scale);
   OXLOG(@"layoutSubviews: bounds=(%.1f,%.1f) scale=%.2f drawableSize=(%.1f,%.1f)", self.bounds.size.width, self.bounds.size.height, scale, layer.drawableSize.width, layer.drawableSize.height);
   EmitWindowMetricsForView(self);
}

- (NSNumber *)ensureIdForTouch:(UITouch *)touch
{
   NSValue *key = [NSValue valueWithNonretainedObject:touch];
   NSNumber *existing = self.touchIds[key];
   if (existing) { return existing; }
   static uint64_t nextId = 1;
   NSNumber *newId = @(nextId++);
   self.touchIds[key] = newId;
   return newId;
}

- (void)removeIdForTouch:(UITouch *)touch
{
   NSValue *key = [NSValue valueWithNonretainedObject:touch];
   [self.touchIds removeObjectForKey:key];
}

- (void)emitTouch:(UITouch *)touch phase:(uint32_t)phase
{
   CGPoint p = [touch locationInView:self];
   CGFloat pressure = 0.0f; uint8_t hasP = 0;
   if (@available(iOS 9.0, *)) { if (touch.maximumPossibleForce > 0.0f) { pressure = touch.force / touch.maximumPossibleForce; hasP = 1; } }
   CGFloat alt = 0.0f, azi = 0.0f; uint8_t hasT = 0;
   if (@available(iOS 9.1, *)) { alt = touch.altitudeAngle; if ([touch respondsToSelector:@selector(azimuthAngleInView:)]) { azi = [touch azimuthAngleInView:self]; hasT = 1; } }
   uint32_t device = 0;
   if (@available(iOS 9.1, *)) { if (touch.type == UITouchTypePencil) device = 1; }
   NSNumber *idNum = [self ensureIdForTouch:touch];
   uint64_t id = idNum.unsignedLongLongValue;
   oxideui_host_emit_touch(id, phase, p.x, p.y, pressure, hasP, alt, azi, hasT, device, ts_now_ns());
   if (phase == 2 || phase == 3) { [self removeIdForTouch:touch]; }
}

- (void)touchesBegan:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event
{
   (void)event; for (UITouch *t in touches) { [self emitTouch:t phase:0]; }
}
- (void)touchesMoved:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event
{
   (void)event; for (UITouch *t in touches) { [self emitTouch:t phase:1]; }
}
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event
{
   (void)event; for (UITouch *t in touches) { [self emitTouch:t phase:2]; }
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event
{
   (void)event; for (UITouch *t in touches) { [self emitTouch:t phase:3]; }
}

- (void)onHover:(UIHoverGestureRecognizer *)rec API_AVAILABLE(ios(13.4))
{
   CGPoint p = [rec locationInView:self]; CGPoint last = self.lastHover; self.lastHover = p;
   float dx = p.x - last.x, dy = p.y - last.y;
   oxideui_host_emit_pointer(p.x, p.y, dx, dy, 0, 0, ts_now_ns());
}

- (void)onPinch:(UIPinchGestureRecognizer *)rec
{
   if (rec.state == UIGestureRecognizerStateBegan || rec.state == UIGestureRecognizerStateChanged)
   {
      CGPoint c = [rec locationInView:self];
      CGFloat scale = rec.scale;
      if (scale > 0.0f)
      {
         float delta = log2f(scale);
         oxideui_host_emit_pinch(c.x, c.y, delta);
      }
      rec.scale = 1.0f;
   }
}

- (void)onDoubleTap:(UITapGestureRecognizer *)rec
{
   if (rec.state == UIGestureRecognizerStateRecognized)
   {
      oxideui_host_emit_double_tap();
   }
}

@end

@interface RustSceneDelegate : UIResponder <UIWindowSceneDelegate, UITextViewDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) CADisplayLink *displayLink;
@property(nonatomic, strong) UILabel *fpsLabel;
@property(nonatomic) CFTimeInterval fpsLastSample;
@property(nonatomic) NSUInteger fpsCount;
@property(nonatomic, strong) UISegmentedControl *sceneControl;
@property(nonatomic, strong) UISwitch *overlaySwitch;
@property(nonatomic, strong) UISwitch *reduceSwitch;
@property(nonatomic, strong) UISwitch *camBlurSwitch;
@property(nonatomic, strong) UISwitch *camGraySwitch;
@property(nonatomic, strong) UISwitch *camAnimSwitch;
@property(nonatomic, strong) UISwitch *camCaptureSwitch;
@property(nonatomic, strong) UISlider *camSigmaSlider;
@property(nonatomic, strong) UISwitch *animPlaySwitch;
@property(nonatomic, strong) UISlider *animPhaseSlider;
@property(nonatomic, strong) UISwitch *damageEnableSwitch;
@property(nonatomic, strong) UISlider *damageUseSlider;
@property(nonatomic, strong) UISlider *damagePrefSlider;
@property(nonatomic, strong) UISlider *nineSliceSlider;
@property(nonatomic, strong) UISlider *nineAlphaSlider;
@property(nonatomic, strong) UISlider *sdfSlider;
@property(nonatomic, strong) UIButton *snapshotButton;
@property(nonatomic, strong) UILabel *statusLabel;
@property(nonatomic, strong) UITextView *imeTextView;
@property(nonatomic, strong) UILabel *camMetricsLabel;
@property(nonatomic) BOOL hasRealScenes;
- (IBAction)sceneChanged:(UISegmentedControl *)control;
- (IBAction)onOverlaySwitch:(UISwitch *)sw;
- (IBAction)onReduceMotionSwitch:(UISwitch *)sw;
- (void)updateDisplayLinkRange;
-(IBAction)onCamBlur:(UISwitch *)sw;
-(IBAction)onCamGray:(UISwitch *)sw;
-(IBAction)onCamAnim:(UISwitch *)sw;
-(IBAction)onCamSigma:(UISlider *)slider;
-(IBAction)onCamCapture:(UISwitch *)sw;
-(IBAction)onImeFocus:(UIButton *)button;
-(IBAction)onImeBlur:(UIButton *)button;
-(IBAction)onImeCopy:(UIButton *)button;
-(IBAction)onImePaste:(UIButton *)button;
-(IBAction)onImeHaptic:(UIButton *)button;
@end

@implementation RustSceneDelegate
- (void)applySceneSelection:(UISegmentedControl *)control
{
   if (!control || !self.hasRealScenes)
   {
      return;
   }
   uint32_t idx = (uint32_t)control.selectedSegmentIndex;
   if (oxideui_host_set_scene(idx) != 0)
   {
      uint32_t current = oxideui_host_current_scene();
      if (current < control.numberOfSegments)
      {
         [control setSelectedSegmentIndex:(NSInteger)current];
      }
   }
}

- (IBAction)sceneChanged:(UISegmentedControl *)control
{
   [self applySceneSelection:control];
   NSInteger idx = control.selectedSegmentIndex;
   NSString *title = (idx >= 0) ? [control titleForSegmentAtIndex:(NSUInteger)idx] : nil;
   if ([title isEqualToString:@"Camera"])
   {
      oxideui_host_set_camera_running(1);
      if (self.camCaptureSwitch)
      {
         [self.camCaptureSwitch setOn:YES animated:YES];
      }
      [self pushCamOptions];
      [self refreshStats];
   }
   else if ([title isEqualToString:@"Animations"])
   {
      [self pushAnimOptions];
   }
   else if ([title isEqualToString:@"Damage Lab"])
   {
      [self pushDamageOptions];
   }
   else if ([title isEqualToString:@"Input & Haptics"])
   {
      [self syncImeState];
      [self pushImeStatus:@"Input scene ready"];
   }
   else if ([title isEqualToString:@"Nine Slice"])
   {
      [self pushNineSliceOptions];
   }
   else if ([title isEqualToString:@"SDF Text"])
   {
      [self pushSdfOptions];
   }
   else if ([title isEqualToString:@"Snapshot"])
   {
      [self refreshSnapshotStatusLabel];
   }

   if (![title isEqualToString:@"Camera"])
   {
      [self refreshStats];
   }
}

- (IBAction)onOverlaySwitch:(UISwitch *)sw
{
   uint8_t desired = sw.isOn ? 1 : 0;
   if (oxideui_host_set_overlay_visible(desired) != 0)
   {
      [sw setOn:!sw.isOn animated:NO];
   }
}

- (IBAction)onReduceMotionSwitch:(UISwitch *)sw
{
   uint8_t desired = sw.isOn ? 1 : 0;
   if (oxideui_host_set_reduce_motion(desired) != 0)
   {
      [sw setOn:!sw.isOn animated:NO];
   }
   [self updateDisplayLinkRange];
}

- (void)pushImeStatus:(NSString *)message
{
   if (!message)
   {
      return;
   }
   const char *utf8 = message.UTF8String;
   if (utf8)
   {
      oxideui_host_input_log(utf8, strlen(utf8));
   }
   if (self.statusLabel)
   {
      self.statusLabel.text = message;
   }
}

- (void)syncImeState
{
   UITextView *tv = self.imeTextView;
   if (!tv)
   {
      return;
   }
   NSRange sel = tv.selectedRange;
   if (sel.location != NSNotFound)
   {
      uint32_t start = (uint32_t)sel.location;
      uint32_t end = (uint32_t)(sel.location + sel.length);
      oxideui_host_emit_text_selection(start, end);
   }
   UITextRange *marked = tv.markedTextRange;
   if (marked)
   {
      NSInteger startIdx = [tv offsetFromPosition:tv.beginningOfDocument toPosition:marked.start];
      NSInteger endIdx = [tv offsetFromPosition:tv.beginningOfDocument toPosition:marked.end];
      NSString *markedText = [tv textInRange:marked] ?: @"";
      NSData *data = [markedText dataUsingEncoding:NSUTF8StringEncoding];
      const char *bytes = data.length > 0 ? data.bytes : NULL;
      oxideui_host_emit_text_composition((uint32_t)startIdx, (uint32_t)endIdx, bytes, (size_t)data.length);
   }
   else
   {
      uint32_t caret = (sel.location != NSNotFound) ? (uint32_t)sel.location : 0;
      oxideui_host_emit_text_composition(caret, caret, NULL, 0);
   }
}

- (void)refreshStats
{
   oxideui_host_stats_t stats = {0};
   if (oxideui_host_app_stats(&stats) != 0)
   {
      return;
   }
   if (self.fpsLabel)
   {
      float dmg = stats.damage_pct * 100.0f;
      self.fpsLabel.text = [NSString stringWithFormat:@"%.0f fps • draws %u • anims %u • damage %.0f%%",
                             stats.fps,
                             stats.draws,
                             stats.anims,
                             dmg];
   }
   if (self.camMetricsLabel)
   {
      NSString *matrix = CameraMatrixName(stats.cam_matrix);
      NSString *range = CameraRangeName(stats.cam_video_range);
      float coverage = stats.cam_coverage_pct * 100.0f;
      float fps = stats.cam_fps;
      BOOL pausedFlag = (stats.cam_paused != 0) || (stats.cam_running == 0);
      NSString *paused = BoolYesNo(pausedFlag);
      NSString *lowPower = BoolYesNo(stats.cam_low_power != 0);
      self.camMetricsLabel.text = [NSString stringWithFormat:@"Cam %ux%u bd=%u mx=%@ rng=%@ cov=%.0f%% fps=%.1f paused=%@ lp=%@ th=%u",
                                    stats.cam_width,
                                    stats.cam_height,
                                    stats.cam_bit_depth,
                                    matrix,
                                    range,
                                    coverage,
                                    fps,
                                    paused,
                                    lowPower,
                                    stats.cam_thermal];
   }
   if (self.camCaptureSwitch)
   {
      BOOL desired = (stats.cam_running != 0) && (stats.cam_paused == 0);
      if (self.camCaptureSwitch.isOn != desired)
      {
         [self.camCaptureSwitch setOn:desired animated:YES];
      }
   }
}

- (BOOL)textView:(UITextView *)textView shouldChangeTextInRange:(NSRange)range replacementText:(NSString *)text
{
   if (textView != self.imeTextView)
   {
      return YES;
   }
   if (range.length > 0)
   {
      for (NSUInteger i = 0; i < range.length; ++i)
      {
         const char backspace = '\b';
         oxideui_host_emit_text_commit(&backspace, 1);
      }
   }
   if (text.length > 0)
   {
      NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
      if (data.length > 0)
      {
         oxideui_host_emit_text_commit(data.bytes, data.length);
      }
   }
   dispatch_async(dispatch_get_main_queue(), ^{ [self syncImeState]; });
   return YES;
}

- (void)textViewDidChangeSelection:(UITextView *)textView
{
   if (textView != self.imeTextView)
   {
      return;
   }
   [self syncImeState];
}

- (void)textViewDidBeginEditing:(UITextView *)textView
{
   if (textView != self.imeTextView)
   {
      return;
   }
   UIView *root = self.window.rootViewController.view;
   UITextRange *sel = textView.selectedTextRange;
   CGRect caret = sel ? [textView caretRectForPosition:sel.start] : CGRectNull;
   if (CGRectIsNull(caret) || CGRectIsEmpty(caret))
   {
      caret = textView.bounds;
   }
   CGRect converted = [textView convertRect:caret toView:root];
   oxideui_host_emit_ime_shown(converted.origin.x, converted.origin.y, converted.size.width, converted.size.height);
   [self pushImeStatus:@"IME shown"];
   [self syncImeState];
}

- (void)textViewDidEndEditing:(UITextView *)textView
{
   if (textView != self.imeTextView)
   {
      return;
   }
   oxideui_host_emit_ime_hidden();
   [self pushImeStatus:@"IME hidden"];
   [self syncImeState];
}

- (void)pushCamOptions
{
   uint8_t blur = self.camBlurSwitch.isOn ? 1 : 0;
   uint8_t gray = self.camGraySwitch.isOn ? 1 : 0;
   uint8_t anim = self.camAnimSwitch.isOn ? 1 : 0;
   float sigma = self.camSigmaSlider.value;
   oxideui_host_set_camera_options(blur, sigma, gray, anim);
   [self refreshStats];
}

-(IBAction)onCamBlur:(UISwitch *)sw { (void)sw; [self pushCamOptions]; }
-(IBAction)onCamGray:(UISwitch *)sw { (void)sw; [self pushCamOptions]; }
-(IBAction)onCamAnim:(UISwitch *)sw { (void)sw; [self pushCamOptions]; }
-(IBAction)onCamSigma:(UISlider *)slider { (void)slider; [self pushCamOptions]; }
(IBAction)onCamCapture:(UISwitch *)sw
{
   uint8_t desired = sw.isOn ? 1 : 0;
   if (oxideui_host_set_camera_running(desired) != 0)
   {
      [sw setOn:!sw.isOn animated:NO];
   }
   [self refreshStats];
}

- (void)pushNineSliceOptions
{
   if (!self.nineSliceSlider || !self.nineAlphaSlider)
   {
      return;
   }
   oxideui_host_set_nine_slice(self.nineSliceSlider.value, self.nineAlphaSlider.value);
}

-(IBAction)onNineSlice:(UISlider *)slider { (void)slider; [self pushNineSliceOptions]; }
-(IBAction)onNineAlpha:(UISlider *)slider { (void)slider; [self pushNineSliceOptions]; }

- (void)pushSdfOptions
{
   if (!self.sdfSlider)
   {
      return;
   }
   oxideui_host_set_sdf_font(self.sdfSlider.value);
}

-(IBAction)onSdfFont:(UISlider *)slider { (void)slider; [self pushSdfOptions]; }

-(IBAction)onImeFocus:(UIButton *)button
{
   (void)button;
   if (self.imeTextView)
   {
      [self.imeTextView becomeFirstResponder];
      [self pushImeStatus:@"IME focus requested"];
      [self syncImeState];
   }
}

-(IBAction)onImeBlur:(UIButton *)button
{
   (void)button;
   if (self.imeTextView)
   {
      [self.imeTextView resignFirstResponder];
      [self pushImeStatus:@"IME blur requested"];
      [self syncImeState];
   }
}

-(IBAction)onImeCopy:(UIButton *)button
{
   (void)button;
   NSString *text = self.imeTextView.text ?: @"";
   NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
   if (data.length > 0)
   {
      oxideui_host_clipboard_set(data.bytes, data.length);
   }
   else
   {
      oxideui_host_clipboard_set("", 0);
   }
   NSString *msg = [NSString stringWithFormat:@"Copied %lu chars", (unsigned long)text.length];
   [self pushImeStatus:msg];
}

-(IBAction)onImePaste:(UIButton *)button
{
   (void)button;
   char *buf = NULL;
   size_t len = 0;
   if (oxideui_host_clipboard_get(&buf, &len))
   {
      NSString *value = [[NSString alloc] initWithBytes:buf length:len encoding:NSUTF8StringEncoding];
      oxideui_host_string_free(buf);
      if (value)
      {
         self.imeTextView.text = value;
         [self syncImeState];
         NSString *msg = [NSString stringWithFormat:@"Pasted %lu chars", (unsigned long)value.length];
         [self pushImeStatus:msg];
         return;
      }
   }
   [self pushImeStatus:@"Paste failed"];
}

-(IBAction)onImeHaptic:(UIButton *)button
{
   (void)button;
   oxideui_host_haptics_play(1);
   [self pushImeStatus:@"Haptic played"];
}

-(void)refreshSnapshotStatusLabelWithFallback:(NSString *)fallback
{
   if (!self.statusLabel)
   {
      return;
   }
   char message[512] = {0};
   uint32_t len = oxideui_host_get_snapshot_status(message, (uint32_t)sizeof(message));
   NSString *status = [NSString stringWithUTF8String:message];
   if (!status || len == 0)
   {
      status = fallback ? fallback : @"";
   }
   self.statusLabel.text = status;
}

-(void)refreshSnapshotStatusLabel
{
   [self refreshSnapshotStatusLabelWithFallback:nil];
}

-(IBAction)onSnapshotButton:(UIButton *)button
{
   (void)button;
   int32_t rc = oxideui_host_take_snapshot();
   NSString *fallback = (rc == 0) ? @"Snapshot saved" : @"Snapshot failed";
   [self refreshSnapshotStatusLabelWithFallback:fallback];
}

- (void)pushAnimOptions
{
   if (!self.animPlaySwitch || !self.animPhaseSlider)
   {
      return;
   }
   oxideui_host_set_anim_play(self.animPlaySwitch.isOn ? 1 : 0);
   oxideui_host_set_anim_progress(self.animPhaseSlider.value);
}

-(IBAction)onAnimPlay:(UISwitch *)sw { (void)sw; [self pushAnimOptions]; }
-(IBAction)onAnimPhase:(UISlider *)slider { (void)slider; [self pushAnimOptions]; }

- (void)pushDamageOptions
{
   if (!self.damageEnableSwitch || !self.damageUseSlider || !self.damagePrefSlider)
   {
      return;
   }
   oxideui_host_set_damage_options(self.damageEnableSwitch.isOn ? 1 : 0,
                                   self.damageUseSlider.value,
                                   self.damagePrefSlider.value);
}

-(IBAction)onDamageEnable:(UISwitch *)sw { (void)sw; [self pushDamageOptions]; }
-(IBAction)onDamageUse:(UISlider *)slider { (void)slider; [self pushDamageOptions]; }
-(IBAction)onDamagePref:(UISlider *)slider { (void)slider; [self pushDamageOptions]; }

- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)connectionOptions
{
   (void)session; (void)connectionOptions;
   if (![scene isKindOfClass:[UIWindowScene class]]) { return; }
   UIWindowScene *ws = (UIWindowScene *)scene;
   self.window = [[UIWindow alloc] initWithWindowScene:ws];
   UIViewController *vc = [UIViewController new];
   MetalView *mv = [MetalView new];
   mv.backgroundColor = [UIColor whiteColor];
   vc.view = mv;
   gMetalView = mv;
   self.window.rootViewController = vc;
    if (!gInFlightDrawables)
    {
        gInFlightDrawables = [NSMutableSet set];
    }

   UILabel *fps = [UILabel new];
   fps.translatesAutoresizingMaskIntoConstraints = NO;
   fps.font = [UIFont monospacedDigitSystemFontOfSize:12 weight:UIFontWeightSemibold];
   fps.textColor = [UIColor colorWithWhite:0.1 alpha:0.95];
   fps.backgroundColor = [UIColor colorWithWhite:1.0 alpha:0.75];
   fps.layer.cornerRadius = 4.0; fps.layer.masksToBounds = YES;
   fps.text = @"-- fps";
   [vc.view addSubview:fps];
   [NSLayoutConstraint activateConstraints:@[
      [fps.topAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.topAnchor constant:8.0],
      [fps.leadingAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.leadingAnchor constant:8.0]
   ]];
   self.fpsLabel = fps;

   // Always show a small on-screen log label during debugging
   UILabel *ll = [UILabel new];
   ll.translatesAutoresizingMaskIntoConstraints = NO;
   ll.font = [UIFont monospacedSystemFontOfSize:9 weight:UIFontWeightRegular];
   ll.textColor = [UIColor colorWithWhite:0.95 alpha:1.0];
   ll.backgroundColor = [[UIColor blackColor] colorWithAlphaComponent:0.6];
   ll.numberOfLines = 0;
   ll.layer.cornerRadius = 4.0; ll.layer.masksToBounds = YES;
   ll.text = @"UILog ready";
   [vc.view addSubview:ll];
   [NSLayoutConstraint activateConstraints:@[
      [ll.leadingAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.leadingAnchor constant:8.0],
      [ll.bottomAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.bottomAnchor constant:-8.0],
      [ll.widthAnchor constraintLessThanOrEqualToConstant:320.0]
   ]];
   gUILogLabel = ll;

  NSMutableArray<NSString *> *sceneItems = [NSMutableArray array];
  uint32_t sceneCount = oxideui_host_scene_count();
  for (uint32_t i = 0; i < sceneCount; ++i)
  {
      uint32_t need = oxideui_host_scene_name(i, NULL, 0);
      if (need == 0) { continue; }
      NSMutableData *buf = [NSMutableData dataWithLength:need];
      if (oxideui_host_scene_name(i, buf.mutableBytes, need) != 0)
      {
         NSString *name = [NSString stringWithUTF8String:buf.mutableBytes];
         if (name) { [sceneItems addObject:name]; }
      }
  }
  self.hasRealScenes = sceneItems.count > 0;
  if (!self.hasRealScenes)
   {
      NSLog(@"[OxideUI] no scenes reported (count=%u)", sceneCount);
      [sceneItems addObject:@"Default Scene"];
   }
  UISegmentedControl *seg = [[UISegmentedControl alloc] initWithItems:sceneItems];
  seg.translatesAutoresizingMaskIntoConstraints = NO;
  seg.selectedSegmentIndex = (NSInteger)oxideui_host_current_scene();
  if (!self.hasRealScenes)
   {
      seg.enabled = NO;
   }
   if (@available(iOS 14.0, *))
   {
      __weak typeof(self) weakSelf = self;
      __weak UISegmentedControl *weakSeg = seg;
      UIAction *action = [UIAction actionWithHandler:^(__kindof UIAction * _Nonnull sceneAction) {
         (void)sceneAction;
         typeof(self) strongSelf = weakSelf;
         UISegmentedControl *strongSeg = weakSeg;
         if (!strongSelf || !strongSeg)
         {
            return;
         }
         [strongSelf sceneChanged:strongSeg];
      }];
      [seg addAction:action forControlEvents:UIControlEventValueChanged];
   }
   else
   {
      [seg addTarget:self action:@selector(sceneChanged:) forControlEvents:UIControlEventValueChanged];
   }
   seg.accessibilityIdentifier = @"sceneControl";
   self.sceneControl = seg;

   UILabel *overlayLabel = [UILabel new];
   overlayLabel.text = @"Overlay";
   overlayLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISwitch *overlaySwitch = [UISwitch new];
   overlaySwitch.on = YES;
   [overlaySwitch addTarget:self action:@selector(onOverlaySwitch:) forControlEvents:UIControlEventValueChanged];
   overlaySwitch.accessibilityIdentifier = @"overlaySwitch";
   self.overlaySwitch = overlaySwitch;

   UILabel *reduceLabel = [UILabel new];
   reduceLabel.text = @"Reduce Motion";
   reduceLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISwitch *reduceSwitch = [UISwitch new];
   reduceSwitch.on = NO;
   [reduceSwitch addTarget:self action:@selector(onReduceMotionSwitch:) forControlEvents:UIControlEventValueChanged];
   reduceSwitch.accessibilityIdentifier = @"reduceMotionSwitch";
   self.reduceSwitch = reduceSwitch;

   UIStackView *overlayRow = [[UIStackView alloc] initWithArrangedSubviews:@[overlayLabel, overlaySwitch]];
   overlayRow.axis = UILayoutConstraintAxisHorizontal;
   overlayRow.spacing = 6.0;

  UIStackView *reduceRow = [[UIStackView alloc] initWithArrangedSubviews:@[reduceLabel, reduceSwitch]];
  reduceRow.axis = UILayoutConstraintAxisHorizontal;
  reduceRow.spacing = 6.0;

   UILabel *inputLabel = [UILabel new];
   inputLabel.text = @"IME Text";
   inputLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UITextView *imeView = [UITextView new];
   imeView.delegate = self;
   imeView.font = [UIFont systemFontOfSize:15 weight:UIFontWeightRegular];
   imeView.layer.borderWidth = 1.0;
   imeView.layer.borderColor = [UIColor colorWithWhite:0.8 alpha:1.0].CGColor;
   imeView.layer.cornerRadius = 4.0;
   imeView.accessibilityIdentifier = @"imeTextView";
   imeView.backgroundColor = [UIColor colorWithWhite:1.0 alpha:0.95];
   imeView.text = @"";
   imeView.translatesAutoresizingMaskIntoConstraints = NO;
   [imeView.heightAnchor constraintEqualToConstant:80.0].active = YES;
   self.imeTextView = imeView;

   UIButton *imeFocus = [UIButton buttonWithType:UIButtonTypeSystem];
   [imeFocus setTitle:@"Focus" forState:UIControlStateNormal];
   imeFocus.accessibilityIdentifier = @"imeFocusButton";
   [imeFocus addTarget:self action:@selector(onImeFocus:) forControlEvents:UIControlEventTouchUpInside];

   UIButton *imeBlur = [UIButton buttonWithType:UIButtonTypeSystem];
   [imeBlur setTitle:@"Blur" forState:UIControlStateNormal];
   imeBlur.accessibilityIdentifier = @"imeBlurButton";
   [imeBlur addTarget:self action:@selector(onImeBlur:) forControlEvents:UIControlEventTouchUpInside];

   UIButton *imeCopy = [UIButton buttonWithType:UIButtonTypeSystem];
   [imeCopy setTitle:@"Copy" forState:UIControlStateNormal];
   imeCopy.accessibilityIdentifier = @"imeCopyButton";
   [imeCopy addTarget:self action:@selector(onImeCopy:) forControlEvents:UIControlEventTouchUpInside];

   UIButton *imePaste = [UIButton buttonWithType:UIButtonTypeSystem];
   [imePaste setTitle:@"Paste" forState:UIControlStateNormal];
   imePaste.accessibilityIdentifier = @"imePasteButton";
   [imePaste addTarget:self action:@selector(onImePaste:) forControlEvents:UIControlEventTouchUpInside];

   UIButton *imeHaptic = [UIButton buttonWithType:UIButtonTypeSystem];
   [imeHaptic setTitle:@"Haptic" forState:UIControlStateNormal];
   imeHaptic.accessibilityIdentifier = @"imeHapticButton";
   [imeHaptic addTarget:self action:@selector(onImeHaptic:) forControlEvents:UIControlEventTouchUpInside];

   UIStackView *imeButtons = [[UIStackView alloc] initWithArrangedSubviews:@[imeFocus, imeBlur, imeCopy, imePaste, imeHaptic]];
   imeButtons.axis = UILayoutConstraintAxisHorizontal;
   imeButtons.spacing = 6.0;

   UIStackView *inputGroup = [[UIStackView alloc] initWithArrangedSubviews:@[inputLabel, imeView, imeButtons]];
   inputGroup.axis = UILayoutConstraintAxisVertical;
   inputGroup.spacing = 4.0;
   inputGroup.alignment = UIStackViewAlignmentFill;

   UILabel *animPlayLabel = [UILabel new];
   animPlayLabel.text = @"Anim Play";
   animPlayLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISwitch *animPlay = [UISwitch new];
   animPlay.on = YES;
   [animPlay addTarget:self action:@selector(onAnimPlay:) forControlEvents:UIControlEventValueChanged];
   animPlay.accessibilityIdentifier = @"animationPlaySwitch";
   self.animPlaySwitch = animPlay;

   UILabel *animPhaseLabel = [UILabel new];
   animPhaseLabel.text = @"Anim Phase";
   animPhaseLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *animPhase = [UISlider new];
   animPhase.minimumValue = 0.0f;
   animPhase.maximumValue = 1.0f;
   animPhase.value = 0.0f;
   [animPhase addTarget:self action:@selector(onAnimPhase:) forControlEvents:UIControlEventValueChanged];
   animPhase.accessibilityIdentifier = @"animationPhaseSlider";
   self.animPhaseSlider = animPhase;

   UIStackView *animRow1 = [[UIStackView alloc] initWithArrangedSubviews:@[animPlayLabel, animPlay]];
   animRow1.axis = UILayoutConstraintAxisHorizontal;
   animRow1.spacing = 6.0;

   UIStackView *animRow2 = [[UIStackView alloc] initWithArrangedSubviews:@[animPhaseLabel, animPhase]];
   animRow2.axis = UILayoutConstraintAxisHorizontal;
   animRow2.spacing = 6.0;

   UILabel *damageLabel = [UILabel new];
   damageLabel.text = @"Damage";
   damageLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISwitch *damageSwitch = [UISwitch new];
   damageSwitch.on = NO;
   [damageSwitch addTarget:self action:@selector(onDamageEnable:) forControlEvents:UIControlEventValueChanged];
   damageSwitch.accessibilityIdentifier = @"damageEnableSwitch";
   self.damageEnableSwitch = damageSwitch;

   UILabel *damageUseLabel = [UILabel new];
   damageUseLabel.text = @"Use Thresh";
   damageUseLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *damageUse = [UISlider new];
   damageUse.minimumValue = 0.0f;
   damageUse.maximumValue = 1.0f;
   damageUse.value = 0.70f;
   [damageUse addTarget:self action:@selector(onDamageUse:) forControlEvents:UIControlEventValueChanged];
   damageUse.accessibilityIdentifier = @"damageUseSlider";
   self.damageUseSlider = damageUse;

   UILabel *damagePrefLabel = [UILabel new];
   damagePrefLabel.text = @"Prefilter";
   damagePrefLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *damagePref = [UISlider new];
   damagePref.minimumValue = 0.0f;
   damagePref.maximumValue = 1.0f;
   damagePref.value = 0.25f;
   [damagePref addTarget:self action:@selector(onDamagePref:) forControlEvents:UIControlEventValueChanged];
   damagePref.accessibilityIdentifier = @"damagePrefSlider";
   self.damagePrefSlider = damagePref;

   UIStackView *damageRow0 = [[UIStackView alloc] initWithArrangedSubviews:@[damageLabel, damageSwitch]];
   damageRow0.axis = UILayoutConstraintAxisHorizontal;
   damageRow0.spacing = 6.0;

   UIStackView *damageRow1 = [[UIStackView alloc] initWithArrangedSubviews:@[damageUseLabel, damageUse]];
   damageRow1.axis = UILayoutConstraintAxisHorizontal;
   damageRow1.spacing = 6.0;

   UIStackView *damageRow2 = [[UIStackView alloc] initWithArrangedSubviews:@[damagePrefLabel, damagePref]];
   damageRow2.axis = UILayoutConstraintAxisHorizontal;
   damageRow2.spacing = 6.0;

   UILabel *nineLabel = [UILabel new];
   nineLabel.text = @"Nine Slice";
   nineLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *nineSlice = [UISlider new];
   nineSlice.minimumValue = 0.0f;
   nineSlice.maximumValue = 40.0f;
   nineSlice.value = 16.0f;
   [nineSlice addTarget:self action:@selector(onNineSlice:) forControlEvents:UIControlEventValueChanged];
   nineSlice.accessibilityIdentifier = @"nineSliceSlider";
   self.nineSliceSlider = nineSlice;

   UILabel *nineAlphaLabel = [UILabel new];
   nineAlphaLabel.text = @"NS Alpha";
   nineAlphaLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *nineAlpha = [UISlider new];
   nineAlpha.minimumValue = 0.1f;
   nineAlpha.maximumValue = 1.0f;
   nineAlpha.value = 1.0f;
   [nineAlpha addTarget:self action:@selector(onNineAlpha:) forControlEvents:UIControlEventValueChanged];
   nineAlpha.accessibilityIdentifier = @"nineAlphaSlider";
   self.nineAlphaSlider = nineAlpha;

   UIStackView *nineRow1 = [[UIStackView alloc] initWithArrangedSubviews:@[nineLabel, nineSlice]];
   nineRow1.axis = UILayoutConstraintAxisHorizontal;
   nineRow1.spacing = 6.0;

   UIStackView *nineRow2 = [[UIStackView alloc] initWithArrangedSubviews:@[nineAlphaLabel, nineAlpha]];
   nineRow2.axis = UILayoutConstraintAxisHorizontal;
   nineRow2.spacing = 6.0;

   UILabel *sdfLabel = [UILabel new];
   sdfLabel.text = @"SDF Font";
   sdfLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *sdfSlider = [UISlider new];
   sdfSlider.minimumValue = 16.0f;
   sdfSlider.maximumValue = 72.0f;
   sdfSlider.value = 32.0f;
   [sdfSlider addTarget:self action:@selector(onSdfFont:) forControlEvents:UIControlEventValueChanged];
   sdfSlider.accessibilityIdentifier = @"sdfFontSlider";
   self.sdfSlider = sdfSlider;

   UIStackView *sdfRow = [[UIStackView alloc] initWithArrangedSubviews:@[sdfLabel, sdfSlider]];
   sdfRow.axis = UILayoutConstraintAxisHorizontal;
   sdfRow.spacing = 6.0;

   UIButton *snapshotButton = [UIButton buttonWithType:UIButtonTypeSystem];
   [snapshotButton setTitle:@"Capture Snapshot" forState:UIControlStateNormal];
   snapshotButton.accessibilityIdentifier = @"snapshotButton";
   [snapshotButton addTarget:self action:@selector(onSnapshotButton:) forControlEvents:UIControlEventTouchUpInside];
   self.snapshotButton = snapshotButton;

   UIStackView *snapshotRow = [[UIStackView alloc] initWithArrangedSubviews:@[snapshotButton]];
   snapshotRow.axis = UILayoutConstraintAxisHorizontal;
   snapshotRow.spacing = 6.0;

   // Camera controls (always visible for UITests)
   UILabel *camLabel = [UILabel new];
   camLabel.text = @"Camera";
   camLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISwitch *camBlur = [UISwitch new];
   camBlur.on = YES;
   [camBlur addTarget:self action:@selector(onCamBlur:) forControlEvents:UIControlEventValueChanged];
   camBlur.accessibilityIdentifier = @"cameraBlurSwitch";
   self.camBlurSwitch = camBlur;
   UISwitch *camGray = [UISwitch new];
   camGray.on = NO;
   [camGray addTarget:self action:@selector(onCamGray:) forControlEvents:UIControlEventValueChanged];
   camGray.accessibilityIdentifier = @"cameraGraySwitch";
   self.camGraySwitch = camGray;
   UISwitch *camAnim = [UISwitch new];
   camAnim.on = YES;
   [camAnim addTarget:self action:@selector(onCamAnim:) forControlEvents:UIControlEventValueChanged];
   camAnim.accessibilityIdentifier = @"cameraAnimateSwitch";
   self.camAnimSwitch = camAnim;
   UISwitch *camCapture = [UISwitch new];
   camCapture.on = YES;
   [camCapture addTarget:self action:@selector(onCamCapture:) forControlEvents:UIControlEventValueChanged];
   camCapture.accessibilityIdentifier = @"cameraCaptureSwitch";
   self.camCaptureSwitch = camCapture;
   UILabel *sigmaLbl = [UILabel new];
   sigmaLbl.text = @"Sigma";
   sigmaLbl.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   UISlider *sigma = [UISlider new];
   sigma.minimumValue = 0.0f;
   sigma.maximumValue = 16.0f;
   sigma.value = 6.0f;
   [sigma addTarget:self action:@selector(onCamSigma:) forControlEvents:UIControlEventValueChanged];
   sigma.accessibilityIdentifier = @"cameraSigmaSlider";
   self.camSigmaSlider = sigma;

   UIStackView *camRow1 = [[UIStackView alloc] initWithArrangedSubviews:@[camLabel, camCapture, camBlur, camGray, camAnim]];
   camRow1.axis = UILayoutConstraintAxisHorizontal;
   camRow1.spacing = 6.0;
   UIStackView *camRow2 = [[UIStackView alloc] initWithArrangedSubviews:@[sigmaLbl, sigma]];
   camRow2.axis = UILayoutConstraintAxisHorizontal;
   camRow2.spacing = 6.0;

   UILabel *camMetrics = [UILabel new];
   camMetrics.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
   camMetrics.textColor = [UIColor colorWithWhite:0.15 alpha:1.0];
   camMetrics.numberOfLines = 2;
   camMetrics.text = @"Cam 0x0 bd=0 mx=709 rng=full cov=0% fps=0.0 paused=yes";
   camMetrics.accessibilityIdentifier = @"cameraMetricsLabel";
   self.camMetricsLabel = camMetrics;
   UIStackView *camMetricsRow = [[UIStackView alloc] initWithArrangedSubviews:@[camMetrics]];
   camMetricsRow.axis = UILayoutConstraintAxisHorizontal;
   camMetricsRow.spacing = 0.0;

  UIStackView *controls = [[UIStackView alloc] initWithArrangedSubviews:@[seg, overlayRow, reduceRow, inputGroup, animRow1, animRow2, damageRow0, damageRow1, damageRow2, nineRow1, nineRow2, sdfRow, snapshotRow, camRow1, camRow2, camMetricsRow]];
  controls.axis = UILayoutConstraintAxisVertical;
  controls.spacing = 8.0;
  controls.alignment = UIStackViewAlignmentLeading;
  controls.translatesAutoresizingMaskIntoConstraints = NO;
  [vc.view addSubview:controls];
  [NSLayoutConstraint activateConstraints:@[
      [controls.leadingAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.leadingAnchor constant:12.0],
      [controls.trailingAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.trailingAnchor constant:-12.0],
      [controls.topAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.topAnchor constant:12.0],
   ]];
  UILabel *status = [UILabel new];
  status.translatesAutoresizingMaskIntoConstraints = NO;
  status.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  status.textColor = [UIColor colorWithWhite:0.15 alpha:1.0];
  status.text = @"";
  status.numberOfLines = 2;
  status.accessibilityIdentifier = @"statusLabel";
  [vc.view addSubview:status];
  [NSLayoutConstraint activateConstraints:@[
      [status.topAnchor constraintEqualToAnchor:controls.bottomAnchor constant:12.0],
      [status.leadingAnchor constraintEqualToAnchor:controls.leadingAnchor],
      [status.trailingAnchor constraintLessThanOrEqualToAnchor:controls.trailingAnchor]
   ]];
  self.statusLabel = status;
  [self.window makeKeyAndVisible];

  [self pushAnimOptions];
  [self pushDamageOptions];
  [self pushNineSliceOptions];
  [self pushSdfOptions];
  [self pushCamOptions];
  [self refreshStats];

   [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(onPowerStateChanged:) name:NSProcessInfoPowerStateDidChangeNotification object:nil];
   if (ShouldRender())
   {
      OXLOG(@"willConnect: creating DisplayLink");
      self.displayLink = [CADisplayLink displayLinkWithTarget:self selector:@selector(onTick:)];
      [self updateDisplayLinkRange];
      [self.displayLink addToRunLoop:[NSRunLoop mainRunLoop] forMode:NSRunLoopCommonModes];
      EnsureHostInitialized(mv);
      [self pushCamOptions];
   }
   else
   {
      OXLOG(@"willConnect: running under UITest — no DisplayLink");
   }
}

- (void)updateDisplayLinkRange
{
   if (!self.displayLink)
   {
      return;
   }
   int fps = CurrentTargetFPS();
   if (@available(iOS 15.0, *))
   {
      self.displayLink.preferredFrameRateRange = CAFrameRateRangeMake(fps, fps, fps);
   }
   else
   {
      self.displayLink.preferredFramesPerSecond = fps;
   }
}

- (void)onPowerStateChanged:(NSNotification *)note
{
   (void)note;
   [self updateDisplayLinkRange];
}

- (void)sceneDidBecomeActive:(UIScene *)scene
{
   (void)scene;
   self.fpsLastSample = CACurrentMediaTime();
   self.fpsCount = 0;
   if (!IsRunningUITest())
   {
      OXLOG(@"sceneDidBecomeActive: resuming DisplayLink");
      self.displayLink.paused = NO;
      EnsureHostInitialized(gMetalView);
      self.overlaySwitch.on = oxideui_host_is_overlay_visible() != 0;
      self.reduceSwitch.on = oxideui_host_is_reduce_motion() != 0;
      [self updateDisplayLinkRange];
   }
   else { OXLOG(@"sceneDidBecomeActive: under UITest"); }
   [self refreshStats];
}

- (void)sceneWillResignActive:(UIScene *)scene
{
   (void)scene;
   if (self.displayLink)
   {
      OXLOG(@"sceneWillResignActive: pausing DisplayLink");
      self.displayLink.paused = YES;
   }
}

- (void)sceneDidEnterBackground:(UIScene *)scene
{
   (void)scene;
   if (self.displayLink)
   {
      OXLOG(@"sceneDidEnterBackground: pausing DisplayLink");
      self.displayLink.paused = YES;
   }
   oxideui_host_app_did_enter_background();
}

- (void)sceneWillEnterForeground:(UIScene *)scene
{
   (void)scene;
   if (!self.displayLink)
   {
      OXLOG(@"sceneWillEnterForeground: no DisplayLink");
      return;
   }
   OXLOG(@"sceneWillEnterForeground: resuming DisplayLink");
   self.displayLink.paused = NO;
   [self updateDisplayLinkRange];
   oxideui_host_app_will_enter_foreground();
}

- (void)sceneDidDisconnect:(UIScene *)scene
{
   (void)scene;
   OXLOG(@"sceneDidDisconnect: tearing down DisplayLink");
   [self.displayLink invalidate];
   self.displayLink = nil;
   gHostAppReady = NO;
   gInFlightDrawables = nil;
   StopMetalCapture();
   oxideui_host_app_will_terminate();
}

- (void)onTick:(CADisplayLink *)link
{
   if (IsRunningUITest())
   {
      return;
   }
   (void)link;
   OXLOG(@"onTick start (main=%d) hostReady=%d", (int)[NSThread isMainThread], (int)gHostAppReady);
   UILog(@"tick");
   MetalView *view = (MetalView *)gMetalView;
   if (!view)
   {
      OXLOG(@"onTick: no MetalView"); UILog(@"no MetalView");
      return;
   }
   EnsureHostInitialized(view);
   if (!gHostAppReady)
   {
      OXLOG(@"onTick: host not ready"); UILog(@"host not ready");
      return;
   }
   CAMetalLayer *layer = (CAMetalLayer *)view.layer;
   id<CAMetalDrawable> drawable = [layer nextDrawable];
   if (!drawable)
   {
      OXLOG(@"nextDrawable returned nil");
      return;
   }
   if (!gInFlightDrawables)
   {
       gInFlightDrawables = [NSMutableSet set];
   }
    [gInFlightDrawables addObject:drawable];
   OXLOG(@"inflight count=%lu", (unsigned long)gInFlightDrawables.count);
   OXLOG(@"presenting drawable %p (class=%@)", (__bridge void *)drawable, [drawable class]);
   UILog([NSString stringWithFormat:@"present %p", (__bridge void *)drawable]);
    id<MTLTexture> drawableTex = drawable.texture;
   OXLOG(@"drawable.texture %p", (__bridge void *)drawableTex);
   UILog([NSString stringWithFormat:@"tex %p", (__bridge void *)drawableTex]);
   CGSize size = layer.drawableSize;
   CGFloat scale = view.window.screen.nativeScale;
   int32_t rc_frame = oxideui_host_app_frame((uint32_t)lrintf((float)size.width), (uint32_t)lrintf((float)size.height), (float)scale);
   if (rc_frame != 0) { OXLOG(@"app_frame rc=%d", rc_frame); UILog([NSString stringWithFormat:@"frame rc=%d", rc_frame]); }
    // Pass non-retained pointer; lifetime is managed by gInFlightDrawables and Metal
    int32_t rc_present = oxideui_host_app_present((__bridge void *)drawable,
                             (__bridge void *)drawableTex);
    if (rc_present != 0) { OXLOG(@"app_present rc=%d", rc_present); UILog([NSString stringWithFormat:@"present rc=%d", rc_present]); }
   self.fpsCount += 1;
   CFTimeInterval now = CACurrentMediaTime();
   if (now - self.fpsLastSample >= 0.5)
   {
      [self refreshStats];
      self.fpsLastSample = now;
      self.fpsCount = 0;
   }
}

@end

@interface RustAppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation RustAppDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions
{
   (void)application; (void)launchOptions;
   return YES;
}

- (UISceneConfiguration *)application:(UIApplication *)application configurationForConnectingSceneSession:(UISceneSession *)connectingSceneSession options:(UISceneConnectionOptions *)options
{
   (void)application;
   (void)options;
   UISceneConfiguration *config = [UISceneConfiguration configurationWithName:@"RustScene" sessionRole:connectingSceneSession.role];
   config.delegateClass = [RustSceneDelegate class];
   return config;
}

- (void)application:(UIApplication *)application didDiscardSceneSessions:(NSSet<UISceneSession *> *)sceneSessions
{
   (void)application;
   (void)sceneSessions;
}

- (void)applicationDidReceiveMemoryWarning:(UIApplication *)application
{
   (void)application;
   oxideui_host_on_memory_warning();
}

- (void)applicationWillTerminate:(UIApplication *)application
{
   (void)application;
   oxideui_host_app_will_terminate();
}

@end

int32_t oxideui_host_start(void)
{
   @autoreleasepool
   {
      OXLOG(@"oxideui_host_start: UIApplicationMain begin");
      const char *argv0 = "oxideui-host";
      char *argv[] = { (char *)argv0, NULL };
      int ret = UIApplicationMain(1, argv, nil, NSStringFromClass([RustAppDelegate class]));
      OXLOG(@"UIApplicationMain returned: %d", ret);
      return ret;
   }
}
static void UILog(NSString *line)
{
   dispatch_on_main(^{
      if (!gUILogLabel || !gUILogLabel.superview) { return; }
      NSString *prev = gUILogLabel.text ?: @"";
      NSString *next = (prev.length > 0) ? [prev stringByAppendingFormat:@"\n%@", line] : line;
      if (next.length > 2000)
      {
         next = [next substringFromIndex:(next.length - 2000)];
      }
      gUILogLabel.text = next;
   });
}
// Bridge for Rust logs
void oxideui_host_ios_log(const char *utf8, size_t len)
{
   if (!utf8 || len == 0)
   {
      return;
   }
   NSString *s = [[NSString alloc] initWithBytes:utf8 length:len encoding:NSUTF8StringEncoding];
   if (!s)
   {
      s = [NSString stringWithUTF8String:utf8];
   }
   if (!s)
   {
      return;
   }
   NSLog(@"[OxideUI-Rust] %@", s);
   UILog(s);
}
