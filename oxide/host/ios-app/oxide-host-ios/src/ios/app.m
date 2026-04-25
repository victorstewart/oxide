#import <AVFoundation/AVFoundation.h>
#import <AudioToolbox/AudioToolbox.h>
#import <Contacts/Contacts.h>
#import <CoreBluetooth/CoreBluetooth.h>
#import <CoreLocation/CoreLocation.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreMotion/CoreMotion.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <Photos/Photos.h>
#import <QuartzCore/QuartzCore.h>
#import <UIKit/UIKit.h>
#import <UserNotifications/UserNotifications.h>
#include <dispatch/dispatch.h>
#include <limits.h>
#include <math.h>
#import <os/lock.h>
#import <os/log.h>
#import <os/signpost.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// Forward declarations for in-file debug sinks.
static void UILog(NSString *line);
static void OxideTouchFileLog(NSString *line);

static inline void OxLogImpl(NSString *msg) {
  // NSLog to device log
  NSLog(@"[Oxide] %@", msg);
  // os_log for devices that suppress NSLog in UI tests
  if (@available(iOS 12.0, *)) {
    os_log_with_type(OS_LOG_DEFAULT, OS_LOG_TYPE_DEFAULT, "[Oxide] %{public}@",
                     msg);
  }
  // Mirror touch-debug sessions to an app-container file that simctl can read.
  OxideTouchFileLog(msg);
  // Mirror to on-screen UILog overlay if present
  UILog(msg);
}

#ifndef OXLOG
#define OXLOG(fmt, ...)                                                        \
  OxLogImpl([NSString stringWithFormat:(fmt), ##__VA_ARGS__])
#endif

static os_log_t OxidePerfSignpostLog(void) {
  static os_log_t log;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    log = os_log_create("com.oxide.perf", OS_LOG_CATEGORY_POINTS_OF_INTEREST);
  });
  return log;
}

static BOOL OxideHotPathLoggingEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_LOG_HOT_PATH"];
    enabled = env != nil && env.intValue != 0;
  });
  return enabled;
}

static BOOL OxidePerfCameraTracePhasesEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_TRACE_PHASES"];
    enabled = env != nil && env.intValue != 0;
  });
  return enabled;
}

static NSUInteger OxidePerfCameraMaximumDrawableCount(void) {
  static NSUInteger count = 3;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_MAX_DRAWABLE_COUNT"];
    NSInteger parsed = env != nil ? env.integerValue : 3;
    if (parsed < 2) {
      parsed = 2;
    } else if (parsed > 3) {
      parsed = 3;
    }
    count = (NSUInteger)parsed;
  });
  return count;
}

static CGFloat OxidePerfCameraPreviewSurfaceScale(void) {
  static CGFloat scale = 1.0;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE"];
    CGFloat parsed = env != nil ? (CGFloat)env.doubleValue : 1.0;
    if (!isfinite(parsed) || parsed <= 0.0) {
      parsed = 1.0;
    } else if (parsed < 0.25) {
      parsed = 0.25;
    } else if (parsed > 1.0) {
      parsed = 1.0;
    }
    scale = parsed;
  });
  return scale;
}

static double OxidePerfNowMs(void) { return CACurrentMediaTime() * 1000.0; }

static NSString *OxideLaunchArgumentValue(NSString *flag) {
  if (flag.length == 0) {
    return nil;
  }
  NSArray<NSString *> *arguments = NSProcessInfo.processInfo.arguments;
  NSUInteger index = [arguments indexOfObject:flag];
  if (index == NSNotFound || index + 1 >= arguments.count) {
    return nil;
  }
  return arguments[index + 1];
}

static BOOL OxideLaunchArgumentEnabled(NSString *flag) {
  if (flag.length == 0) {
    return NO;
  }
  NSString *value = OxideLaunchArgumentValue(flag);
  if (value != nil) {
    return value.intValue != 0;
  }
  return [NSProcessInfo.processInfo.arguments containsObject:flag];
}

static BOOL OxidePerfCameraRealAppHostEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_REAL_APP_HOST"];
    if (env != nil) {
      enabled = env.intValue != 0;
    } else {
      enabled = OxideLaunchArgumentEnabled(@"-oxide-perf-camera-real-app-host");
    }
  });
  return enabled;
}

static BOOL OxidePerfCameraRealAppHybridVisiblePreviewEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW"];
    enabled = env != nil && env.intValue != 0;
  });
  return enabled;
}

static BOOL OxidePerfCameraFrameDrivenSchedulingEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_FRAME_DRIVEN_SCHEDULING"];
    enabled = env != nil && env.intValue != 0 &&
              OxidePerfCameraRealAppHostEnabled() &&
              !OxidePerfCameraRealAppHybridVisiblePreviewEnabled();
  });
  return enabled;
}

static BOOL OxidePerfCameraNoVisiblePresentEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_NO_VISIBLE_PRESENT"];
    enabled = env != nil && env.intValue != 0;
  });
  return enabled;
}

static NSString *OxidePerfCaseName(void) {
  static NSString *caseName = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env =
        [NSProcessInfo.processInfo.environment objectForKey:@"OXIDE_PERF_CASE"];
    NSString *argumentValue =
        env.length > 0 ? nil : OxideLaunchArgumentValue(@"-oxide-perf-case");
    NSString *resolved = env.length > 0 ? env : argumentValue;
    caseName = resolved.length > 0 ? [resolved copy] : @"";
  });
  return caseName;
}

static BOOL OxidePerfActualAppCustomCameraBenchmarkEnabled(void) {
  return OxidePerfCameraRealAppHostEnabled() &&
         [OxidePerfCaseName()
             isEqualToString:@"testCameraNV12LegacyRealAppLivePreview"];
}

static BOOL OxidePerfActualAppAVFoundationCameraBenchmarkEnabled(void) {
  return OxidePerfCameraRealAppHostEnabled() &&
         [OxidePerfCaseName()
             isEqualToString:
                 @"testCameraAVFoundationPreviewLayerRealAppLivePreview"];
}

static BOOL OxidePerfActualAppCameraBenchmarkEnabled(void) {
  return OxidePerfActualAppCustomCameraBenchmarkEnabled() ||
         OxidePerfActualAppAVFoundationCameraBenchmarkEnabled();
}

static NSString *OxidePerfCameraCaptureContractPresetName(void) {
  static NSString *presetName = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE"];
    NSString *normalized = [[env
        stringByTrimmingCharactersInSet:[NSCharacterSet
                                            whitespaceAndNewlineCharacterSet]]
        lowercaseString];
    if ([normalized isEqualToString:@"preset-720p"] ||
        [normalized isEqualToString:@"preset720p"] ||
        [normalized isEqualToString:@"hd1280x720"]) {
      presetName = @"hd1280x720";
    } else {
      presetName = @"inputPriority";
    }
  });
  return presetName;
}

static void OxidePerfEmitConsoleLine(NSString *line) {
  if (line.length == 0) {
    return;
  }
  fprintf(stdout, "%s\n", line.UTF8String);
  fflush(stdout);
}

static void OxidePerfPostDarwinNotification(NSString *name) {
  if (name.length == 0) {
    return;
  }
  CFNotificationCenterPostNotification(
      CFNotificationCenterGetDarwinNotifyCenter(), (__bridge CFStringRef)name,
      NULL, NULL, true);
}

static void OxidePerfEmitJSONLine(NSString *prefix, NSDictionary *payload) {
  if (prefix.length == 0 || payload == nil) {
    return;
  }
  NSError *error = nil;
  NSData *data = [NSJSONSerialization dataWithJSONObject:payload
                                                 options:NSJSONWritingSortedKeys
                                                   error:&error];
  if (data == nil) {
    OXLOG(@"failed to encode perf JSON %@: %@", prefix, error);
    return;
  }
  NSString *json = [[NSString alloc] initWithData:data
                                         encoding:NSUTF8StringEncoding];
  if (json.length == 0) {
    return;
  }
  OxidePerfEmitConsoleLine([prefix stringByAppendingString:json]);
}

static BOOL OxidePerfSignpostMatches(const char *utf8, size_t len,
                                     const char *literal) {
  if (utf8 == NULL || literal == NULL) {
    return NO;
  }
  size_t literalLen = strlen(literal);
  return literalLen == len && memcmp(utf8, literal, len) == 0;
}

#define OXIDE_PERF_TRY_BEGIN(literal)                                          \
  if (OxidePerfSignpostMatches(utf8, len, literal)) {                          \
    os_signpost_interval_begin(log, signpostId, literal);                      \
    return (uint64_t)signpostId;                                               \
  }

#define OXIDE_PERF_TRY_END(literal)                                            \
  if (OxidePerfSignpostMatches(utf8, len, literal)) {                          \
    os_signpost_interval_end(log, signpostId, literal);                        \
    return;                                                                    \
  }

uint64_t oxide_host_perf_workload_signpost_begin(void) {
  if (@available(iOS 12.0, *)) {
    os_log_t log = OxidePerfSignpostLog();
    os_signpost_id_t signpostId = os_signpost_id_generate(log);
    os_signpost_interval_begin(log, signpostId, "PerfWorkload");
    return (uint64_t)signpostId;
  }
  return 0;
}

void oxide_host_perf_workload_signpost_end(uint64_t signpostIdRaw) {
  if (signpostIdRaw == 0) {
    return;
  }
  if (@available(iOS 12.0, *)) {
    os_log_t log = OxidePerfSignpostLog();
    os_signpost_id_t signpostId = (os_signpost_id_t)signpostIdRaw;
    os_signpost_interval_end(log, signpostId, "PerfWorkload");
  }
}

uint64_t oxide_host_perf_signpost_begin(const char *utf8, size_t len) {
  if (utf8 == NULL || len == 0) {
    return 0;
  }
  if (!OxidePerfCameraTracePhasesEnabled()) {
    return 0;
  }
  if (@available(iOS 12.0, *)) {
    os_log_t log = OxidePerfSignpostLog();
    os_signpost_id_t signpostId = os_signpost_id_generate(log);
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct_preview");
    OXIDE_PERF_TRY_BEGIN("camera.host.present");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.resize");
    OXIDE_PERF_TRY_BEGIN("camera.router.update_draw");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.begin_frame");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.coalesce");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.encode_pass");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.submit");
    OXIDE_PERF_TRY_BEGIN("camera.capture.total");
    OXIDE_PERF_TRY_BEGIN("camera.capture.sample_setup");
    OXIDE_PERF_TRY_BEGIN("camera.capture.lock");
    OXIDE_PERF_TRY_BEGIN("camera.capture.texture_bridge");
    OXIDE_PERF_TRY_BEGIN("camera.capture.publish");
    OXIDE_PERF_TRY_BEGIN("camera.capture.publish.lock");
    OXIDE_PERF_TRY_BEGIN("camera.capture.publish.texture_refs");
    OXIDE_PERF_TRY_BEGIN("camera.capture.publish.pixel_buffer");
    OXIDE_PERF_TRY_BEGIN("camera.capture.frame_delivery");
    OXIDE_PERF_TRY_BEGIN("camera.fetch.live_yuv");
    OXIDE_PERF_TRY_BEGIN("camera.fetch.live_bgra");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.fetch");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.command_buffer");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.encoder");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.setup");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.encode_quad");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.encode.bind");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.encode.draw");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.end_encoding");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.present_drawable");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.commit");
    OXIDE_PERF_TRY_BEGIN("camera.renderer.direct.poll_submissions");
  }
  return 0;
}

void oxide_host_perf_signpost_end(const char *utf8, size_t len,
                                  uint64_t signpostIdRaw) {
  if (utf8 == NULL || len == 0 || signpostIdRaw == 0) {
    return;
  }
  if (!OxidePerfCameraTracePhasesEnabled()) {
    return;
  }
  if (@available(iOS 12.0, *)) {
    os_log_t log = OxidePerfSignpostLog();
    os_signpost_id_t signpostId = (os_signpost_id_t)signpostIdRaw;
    OXIDE_PERF_TRY_END("camera.renderer.direct_preview");
    OXIDE_PERF_TRY_END("camera.host.present");
    OXIDE_PERF_TRY_END("camera.renderer.resize");
    OXIDE_PERF_TRY_END("camera.router.update_draw");
    OXIDE_PERF_TRY_END("camera.renderer.begin_frame");
    OXIDE_PERF_TRY_END("camera.renderer.coalesce");
    OXIDE_PERF_TRY_END("camera.renderer.encode_pass");
    OXIDE_PERF_TRY_END("camera.renderer.submit");
    OXIDE_PERF_TRY_END("camera.capture.total");
    OXIDE_PERF_TRY_END("camera.capture.sample_setup");
    OXIDE_PERF_TRY_END("camera.capture.lock");
    OXIDE_PERF_TRY_END("camera.capture.texture_bridge");
    OXIDE_PERF_TRY_END("camera.capture.publish");
    OXIDE_PERF_TRY_END("camera.capture.publish.lock");
    OXIDE_PERF_TRY_END("camera.capture.publish.texture_refs");
    OXIDE_PERF_TRY_END("camera.capture.publish.pixel_buffer");
    OXIDE_PERF_TRY_END("camera.capture.frame_delivery");
    OXIDE_PERF_TRY_END("camera.fetch.live_yuv");
    OXIDE_PERF_TRY_END("camera.fetch.live_bgra");
    OXIDE_PERF_TRY_END("camera.renderer.direct.fetch");
    OXIDE_PERF_TRY_END("camera.renderer.direct.command_buffer");
    OXIDE_PERF_TRY_END("camera.renderer.direct.encoder");
    OXIDE_PERF_TRY_END("camera.renderer.direct.setup");
    OXIDE_PERF_TRY_END("camera.renderer.direct.encode_quad");
    OXIDE_PERF_TRY_END("camera.renderer.direct.encode.bind");
    OXIDE_PERF_TRY_END("camera.renderer.direct.encode.draw");
    OXIDE_PERF_TRY_END("camera.renderer.direct.end_encoding");
    OXIDE_PERF_TRY_END("camera.renderer.direct.present_drawable");
    OXIDE_PERF_TRY_END("camera.renderer.direct.commit");
    OXIDE_PERF_TRY_END("camera.renderer.direct.poll_submissions");
  }
}

// ===== Structs =====
// OxBleScanConfig moved to shared platform-ios bluetooth.m

typedef struct OxBleScanConfig OxBleScanConfig;

typedef struct {
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

typedef struct {
  const int16_t *audio_ptr;
  size_t sample_count;
  uint32_t channels;
  uint32_t sample_rate_hz;
  uint64_t timestamp_ns;
} OxCameraAudio;

typedef struct {
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
} OxCameraAcquiredFrame;

// ===== Rust FFI declarations (Objective-C -> Rust) =====
void oxide_host_ble_emit_state(uint32_t state);
void oxide_host_ble_emit_discovered(const void *info);
void oxide_host_ble_emit_connected(const uint8_t *addr);
void oxide_host_ble_emit_disconnected(const uint8_t *addr);

void oxide_host_emit_window_resized(float w, float h, float scale, float safe_l,
                                    float safe_t, float safe_r, float safe_b);
void oxide_host_on_memory_warning(void);
void oxide_host_emit_text_commit(const char *utf8, size_t len);
void oxide_host_emit_text_composition(uint32_t start, uint32_t end,
                                      const char *utf8, size_t len);
void oxide_host_emit_text_selection(uint32_t start, uint32_t end);
void oxide_host_emit_ime_shown(float x, float y, float w, float h);
void oxide_host_emit_ime_hidden(void);
void oxide_host_emit_touch(uint64_t id, uint32_t phase, float x, float y,
                           float pressure, uint8_t has_pressure, float tilt_alt,
                           float tilt_azi, uint8_t has_tilt, uint32_t device,
                           uint64_t ts_ns);
void oxide_host_emit_pointer(float x, float y, float dx, float dy,
                             uint32_t buttons, uint32_t modifiers,
                             uint64_t ts_ns);
void oxide_host_emit_key(uint32_t code, const char *chars, size_t chars_len,
                         uint8_t repeat, uint32_t modifiers, uint64_t ts_ns);
void oxide_host_emit_pinch(float cx, float cy, float delta);
void oxide_host_emit_pan_gesture(float x, float y, float dx, float dy,
                                 uint8_t active);
void oxide_host_emit_double_tap(void);
void oxide_host_emit_perm(uint32_t domain, uint32_t status);
void oxide_host_emit_push_token(uint32_t provider, const char *utf8,
                                size_t len);
void oxide_host_emit_push_notify(const char *utf8, size_t len);

int32_t oxide_host_app_init(uint32_t w, uint32_t h, float scale);
int32_t oxide_host_app_frame(uint32_t w, uint32_t h, float scale);
int32_t oxide_host_camera_preview_plan(uint32_t w, uint32_t h, float scale);
int32_t oxide_host_camera_preview_plan_reason(uint32_t w, uint32_t h,
                                              float scale);
int32_t oxide_host_app_frame_with_drawable(uint32_t w, uint32_t h, float scale,
                                           void *drawable_ptr);
int32_t oxide_host_app_stats(void *stats_out);
uint32_t oxide_host_scene_count(void);
uint32_t oxide_host_scene_name(uint32_t idx, char *out_ptr, uint32_t out_len);
uint32_t oxide_host_current_scene(void);
int32_t oxide_host_set_scene(uint32_t idx);
uint8_t oxide_host_is_overlay_visible(void);
int32_t oxide_host_set_overlay_visible(uint8_t on);
uint8_t oxide_host_is_reduce_motion(void);
int32_t oxide_host_set_reduce_motion(uint8_t on);
void oxide_host_app_did_enter_background(void);
void oxide_host_app_will_enter_foreground(void);
void oxide_host_app_will_terminate(void);
void oxide_host_on_memory_warning(void);

// ===== Rust FFI declarations (Rust -> Objective-C) =====
void oxide_host_clipboard_set(const char *utf8, size_t len);
int oxide_host_clipboard_get(char **out_ptr, size_t *out_len);
void oxide_host_string_free(char *p);
void oxide_host_haptics_play(uint32_t pattern);
uint32_t oxide_host_perm_status(uint32_t domain);
void oxide_host_perm_request(uint32_t domain);
void oxide_host_push_register(void);
int oxide_host_push_get_device_token(char **out_ptr, size_t *out_len);
void oxide_host_push_set_badge(int32_t count);
void oxide_host_push_clear_badge(void);
void oxide_host_push_bootstrap(void);
void oxide_host_push_application_did_register(NSData *deviceToken);
void oxide_host_push_application_did_fail(NSError *error);
void oxide_host_push_application_did_receive(NSDictionary *userInfo);
void oxide_host_input_log(const char *utf8, size_t len);
int32_t oxide_host_set_benchmark_mode(uint8_t on);
int32_t oxide_host_set_camera_running(uint8_t running);
int32_t oxide_host_set_camera_running_mode(uint8_t running,
                                           uint8_t preview_only);
int32_t oxide_host_set_camera_render_mode(int32_t mode);
int32_t oxide_host_set_camera_texture_source(int32_t source);
int32_t oxide_host_reset_camera_perf_counters(void);
uint8_t oxide_ble_is_supported(void);
void oxide_ble_init(void);
uint8_t oxide_ble_powered_on(void);
void oxide_ble_shutdown(void);
void oxide_ble_start_scan(const OxBleScanConfig *cfg);
void oxide_ble_stop_scan(void);
void oxide_ble_connect(const uint8_t *addr, size_t addr_len);
void oxide_ble_disconnect(const uint8_t *addr, size_t addr_len);
int oxide_ble_read_char(const uint8_t *addr, size_t addr_len,
                        const uint16_t *uuid16);
int oxide_ble_write_char(const uint8_t *addr, size_t addr_len,
                         const uint16_t *uuid16, const uint8_t *data,
                         size_t len);
int oxide_ble_subscribe(const uint8_t *addr, size_t addr_len,
                        const uint16_t *uuid16, uint8_t on);
int32_t oxide_host_resource_read(const char *name, void **out_ptr,
                                 size_t *out_len);
void oxide_host_set_resource_loader(uint8_t (*cb)(const char *, void **,
                                                  size_t *));
void oxide_host_ime_show(void);
void oxide_host_ime_hide(void);
int32_t oxide_host_start(int argc, char **argv);

int32_t oxide_cam_start_default(void);
int32_t oxide_cam_start_default_preview_only(void);
void oxide_cam_stop(void);
int32_t oxide_cam_set_fps(int32_t fps);
int32_t oxide_cam_set_resolution_height(int32_t h);
int32_t oxide_cam_set_bit_depth(int32_t bits);
int32_t oxide_cam_set_color_space(int32_t id);
int32_t oxide_cam_set_position(int32_t pos);
int32_t oxide_cam_set_mode(int32_t mode);
void oxide_host_set_camera_callback(void (*cb)(const OxCameraFrame *));
void oxide_host_set_camera_audio_callback(void (*cb)(const OxCameraAudio *));
int32_t oxide_cam_get_latest(void **y_tex, void **uv_tex, int32_t *w,
                             int32_t *h);
int32_t oxide_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w,
                                int32_t *h, int32_t *bitdepth, int32_t *matrix,
                                int32_t *video_range, int32_t *colorspace);
int32_t oxide_cam_acquire_latest_frame_ex(uint64_t min_generation_exclusive,
                                          OxCameraAcquiredFrame *out_frame);
void *oxide_cam_get_running_session(void);
uint64_t oxide_cam_peek_latest_generation(void);
uint64_t oxide_cam_peek_latest_timestamp_ns(void);
void oxide_cam_release_acquired(uint32_t slot, uint64_t generation);
typedef void (*OxideCameraPreviewPublishCallback)(uint64_t generation,
                                                  uint64_t timestamp_ns,
                                                  void *context);
void oxide_cam_set_preview_publish_callback(
    OxideCameraPreviewPublishCallback callback, void *context);
int32_t oxide_host_power_lowpower(void);
int32_t oxide_host_thermal_state(void);
int32_t oxide_host_set_camera_options(uint8_t blur, float sigma,
                                      uint8_t grayscale, uint8_t animate);
int32_t oxide_host_set_anim_play(uint8_t play);
int32_t oxide_host_set_anim_progress(float normalized);
int32_t oxide_host_set_damage_options(uint8_t enabled, float use_thresh,
                                      float prefilter);
int32_t oxide_host_set_nine_slice(float slice_px, float alpha);
int32_t oxide_host_set_sdf_font(float font_px);
int32_t oxide_host_take_snapshot(void);
uint32_t oxide_host_get_snapshot_status(char *out_ptr, uint32_t out_len);

// ===== Host state =====

typedef struct oxide_host_stats_t {
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
  float cam_poll_submissions_ms;
  float cam_fetch_ms;
  float cam_setup_ms;
  float cam_encode_quad_ms;
  float cam_command_buffer_ms;
  float cam_encoder_ms;
  float cam_encode_bind_ms;
  float cam_encode_draw_ms;
  float cam_end_encoding_ms;
  float cam_present_ms;
  float cam_commit_ms;
  float cam_gpu_ms;
  float cam_gpu_render_ms;
  float cam_gpu_vertex_ms;
  float cam_gpu_fragment_ms;
  float cam_capture_total_ms;
  float cam_capture_sample_setup_ms;
  float cam_capture_lock_ms;
  float cam_capture_texture_bridge_ms;
  float cam_capture_publish_ms;
  float cam_capture_publish_lock_ms;
  float cam_capture_publish_texture_refs_ms;
  float cam_capture_publish_pixel_buffer_ms;
  float cam_capture_frame_delivery_ms;
  uint64_t cam_sample_delivery_pool_bytes;
  uint32_t cam_sample_delivery_pool_surfaces;
  uint64_t cam_active_sample_surface_bytes;
  uint32_t cam_active_sample_surface_surfaces;
  uint32_t cam_active_sample_buffers;
  uint64_t cam_peak_active_sample_surface_bytes;
  uint32_t cam_peak_active_sample_surface_surfaces;
  uint32_t cam_peak_active_sample_buffers;
  uint32_t cam_sample_delivery_total_samples;
  uint32_t cam_sample_delivery_reused_frames;
  uint32_t cam_sample_delivery_reused_surfaces;
  uint32_t cam_sample_delivery_max_reuse_gap_frames;
  uint64_t cam_retained_sample_surface_bytes;
  uint32_t cam_retained_sample_surface_surfaces;
  uint64_t cam_retained_published_slot_surface_bytes;
  uint32_t cam_retained_published_slot_surfaces;
  uint64_t cam_retained_latest_pixel_buffer_surface_bytes;
  uint32_t cam_retained_latest_pixel_buffer_surface_surfaces;
  uint64_t cam_latest_published_generation;
  uint64_t cam_latest_published_timestamp_ns;
  uint64_t cam_latest_presented_generation;
  uint32_t cam_generation_advances;
  uint32_t cam_samples_received;
  uint32_t cam_samples_dropped_prebridge;
  uint32_t cam_samples_bridged;
  uint32_t cam_samples_published;
  uint32_t cam_samples_presented;
  uint32_t cam_samples_superseded_before_present;
  uint64_t renderer_memory_total_bytes;
  uint64_t renderer_memory_draw_targets_bytes;
  uint64_t renderer_memory_draw_target_main_bytes;
  uint64_t renderer_memory_draw_target_msaa_bytes;
  uint64_t renderer_memory_effect_targets_bytes;
  uint64_t renderer_memory_effect_prepass_bytes;
  uint64_t renderer_memory_effect_blur_chain_bytes;
  uint64_t renderer_memory_live_camera_bytes;
  uint64_t renderer_memory_camera_cache_bytes;
  uint64_t renderer_memory_camera_blur_cache_bytes;
  uint64_t renderer_memory_camera_transition_cache_bytes;
  uint64_t renderer_memory_benchmark_camera_bytes;
  uint64_t renderer_memory_layer_cache_bytes;
  uint64_t renderer_memory_image_cache_bytes;
  uint64_t renderer_memory_buffer_bytes;
  uint32_t renderer_pending_command_buffers;
  uint32_t renderer_pending_present_drawables;
  uint32_t renderer_pending_present_textures;
  uint32_t renderer_preview_submission_depth;
  uint32_t renderer_preview_submission_skipped;
  float renderer_preview_submission_frame_age_ms;
} oxide_host_stats_t;

typedef struct oxide_host_camera_tick_perf_t {
  uint64_t serial;
  uint32_t drawable_width;
  uint32_t drawable_height;
  float drawable_scale;
  uint32_t plan_reason;
  float plan_ms;
  float drawable_acquire_ms;
  float frame_call_ms;
  float tick_total_ms;
  uint8_t skipped;
  uint8_t drawable_acquired;
  uint8_t frame_submitted;
  uint8_t reserved;
} oxide_host_camera_tick_perf_t;

typedef struct oxide_host_app_debug_perf_t {
  uint32_t scene_will_connect_calls;
  uint32_t perf_scene_branch_calls;
  uint32_t normal_scene_branch_calls;
  uint32_t metal_view_installs;
  uint32_t display_link_create_calls;
  uint32_t scene_did_become_active_calls;
  uint32_t scene_will_enter_foreground_calls;
  uint32_t ensure_host_initialized_calls;
  uint32_t host_ready_transitions;
  uint32_t on_tick_calls;
  uint32_t camera_frame_triggered_renders;
  uint32_t plan_skips;
  uint32_t drawables_acquired;
  uint32_t command_buffers_committed;
  uint8_t running_ui_test;
  uint8_t running_perf_benchmark_host;
  uint8_t should_render;
  uint8_t host_ready;
} oxide_host_app_debug_perf_t;

__attribute__((weak)) void nametag_host_update_permission(int32_t domain,
                                                          int32_t status) {
  (void)domain;
  (void)status;
}

__attribute__((weak)) void
nametag_ios_handle_notification_response(NSDictionary *userInfo) {
  (void)userInfo;
}

@class RustSceneDelegate;
static __weak UIView *gMetalView = nil;
static __weak RustSceneDelegate *gActiveRustSceneDelegate = nil;
static BOOL gHostAppReady = NO;
static id<MTLDevice> gMetalDevice = nil;
static UILabel *gUILogLabel = nil;
static oxide_host_camera_tick_perf_t gLastCameraTickPerf = {0};
static oxide_host_app_debug_perf_t gAppDebugPerf = {0};
static _Atomic(uint8_t) gCameraPreviewNeedsPresent = 0;

int32_t oxide_host_camera_tick_perf(void *tick_out) {
  if (tick_out == NULL) {
    return -1;
  }
  *(oxide_host_camera_tick_perf_t *)tick_out = gLastCameraTickPerf;
  return 0;
}

int32_t oxide_host_app_debug_perf(void *debug_out) {
  if (debug_out == NULL) {
    return -1;
  }
  oxide_host_app_debug_perf_t snapshot = gAppDebugPerf;
  snapshot.host_ready = gHostAppReady ? 1 : 0;
  *(oxide_host_app_debug_perf_t *)debug_out = snapshot;
  return 0;
}

int32_t oxide_host_reset_app_debug_perf(void) {
  gAppDebugPerf = (oxide_host_app_debug_perf_t){0};
  gLastCameraTickPerf = (oxide_host_camera_tick_perf_t){0};
  atomic_store_explicit(&gCameraPreviewNeedsPresent, 0, memory_order_release);
  return 0;
}

static void StartMetalCaptureIfEnabled(id<MTLDevice> dev) {
  const char *env = getenv("OXIDE_CAPTURE_METAL");
  if (!env) {
    return;
  }
  if (!(strcmp(env, "1") == 0 || strcasecmp(env, "true") == 0)) {
    return;
  }
  if (@available(iOS 13.0, *)) {
    MTLCaptureManager *mgr = [MTLCaptureManager sharedCaptureManager];
    if (mgr.isCapturing) {
      return;
    }
    MTLCaptureDescriptor *desc = [MTLCaptureDescriptor new];
    desc.captureObject = dev;
    desc.destination = MTLCaptureDestinationDeveloperTools;
    NSError *err = nil;
    [mgr startCaptureWithDescriptor:desc error:&err];
    if (err) {
      OXLOG(@"MTLCapture start error: %@", err.localizedDescription);
    } else {
      OXLOG(@"MTLCapture started (device=%p)", dev);
    }
  }
}

static void StopMetalCapture(void) {
  if (@available(iOS 13.0, *)) {
    MTLCaptureManager *mgr = [MTLCaptureManager sharedCaptureManager];
    if (mgr.isCapturing) {
      [mgr stopCapture];
      OXLOG(@"MTLCapture stopped");
    }
  }
}

static BOOL IsRunningUITest(void) {
  static BOOL checked = NO;
  static BOOL cached = NO;
  if (!checked) {
    NSArray<NSString *> *arguments = NSProcessInfo.processInfo.arguments;
    NSDictionary<NSString *, NSString *> *env =
        NSProcessInfo.processInfo.environment;
    // Robust detection for XCTest / UI tests
    BOOL hintArg = [arguments containsObject:@"UITEST"] ||
                   [arguments containsObject:@"UITests"]; // project convention
    BOOL hintEnv = [[env objectForKey:@"UITEST"] boolValue];
    BOOL xcCfg = [env objectForKey:@"XCTestConfigurationFilePath"] != nil;
    BOOL xcInject =
        [env objectForKey:@"XCInjectBundleInto"] != nil; // unit tests
    BOOL isTesting = hintArg || hintEnv || xcCfg || xcInject;
    cached = isTesting;
    gAppDebugPerf.running_ui_test = cached ? 1 : 0;
    OXLOG(@"IsRunningUITest? %d (args=%@ envKeys=%@)", (int)cached, arguments,
          env.allKeys);
    checked = YES;
  }
  return cached;
}

static BOOL IsRunningPerfBenchmarkHost(void) {
  static BOOL checked = NO;
  static BOOL cached = NO;
  if (!checked) {
    NSDictionary<NSString *, NSString *> *env =
        NSProcessInfo.processInfo.environment;
    NSString *bundlePath = [env objectForKey:@"XCTestBundlePath"];
    NSString *injectPath = [env objectForKey:@"XCInjectBundleInto"];
    cached =
        (bundlePath != nil &&
         [bundlePath rangeOfString:@"OxideHostPerfTests.xctest"].location !=
         NSNotFound) ||
        (injectPath != nil &&
         [injectPath rangeOfString:@"OxideHostPerfTests.xctest"].location !=
         NSNotFound) ||
        OxidePerfCameraRealAppHostEnabled();
    gAppDebugPerf.running_perf_benchmark_host = cached ? 1 : 0;
    checked = YES;
  }
  return cached;
}

static BOOL ShouldRender(void) {
  // Allow opting into rendering during UI tests via env
  NSDictionary<NSString *, NSString *> *env =
      NSProcessInfo.processInfo.environment;
  NSString *v = [env objectForKey:@"OXIDE_RENDER_IN_TEST"];
  BOOL render_in_test =
      (v && ([v isEqualToString:@"1"] ||
             [[v lowercaseString] isEqualToString:@"true"]))
          ? YES
          : OxideLaunchArgumentEnabled(@"-oxide-render-in-test");
  BOOL should_render = render_in_test || !IsRunningUITest();
  gAppDebugPerf.should_render = should_render ? 1 : 0;
  return should_render;
}

static BOOL OxideHostChromeHidden(void) {
  NSString *value = [NSProcessInfo.processInfo.environment
      objectForKey:@"OXIDE_HIDE_HOST_CHROME"];
  if (value == nil) {
    return NO;
  }
  NSString *normalized = [[value
      stringByTrimmingCharactersInSet:[NSCharacterSet
                                          whitespaceAndNewlineCharacterSet]]
      lowercaseString];
  return [normalized isEqualToString:@"1"] ||
         [normalized isEqualToString:@"true"] ||
         [normalized isEqualToString:@"yes"];
}

static BOOL OxideTouchDebugEnabled(void) {
  static BOOL checked = NO;
  static BOOL enabled = NO;
  if (!checked) {
    NSString *value = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_TOUCH_LOG"];
    enabled = value != nil && value.intValue != 0;
    checked = YES;
  }
  return enabled;
}

static BOOL OxideTouchScreenLogEnabled(void) {
  static BOOL checked = NO;
  static BOOL enabled = NO;
  if (!checked) {
    NSString *value = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_TOUCH_LOG_SCREEN"];
    enabled = value != nil && value.intValue != 0;
    checked = YES;
  }
  return enabled;
}

static BOOL OxideWindowTouchCaptureEnabled(void) { return YES; }

static BOOL OxideTouchIsDirect(UITouch *touch) {
  return touch != nil && touch.type == UITouchTypeDirect;
}

static BOOL OxideEventHasOnlyDirectTouches(UIEvent *event) {
  NSSet<UITouch *> *touches = event.allTouches;
  if (touches.count == 0) {
    return NO;
  }
  for (UITouch *touch in touches) {
    if (!OxideTouchIsDirect(touch)) {
      return NO;
    }
  }
  return YES;
}

static BOOL OxideTouchPhaseForUITouch(UITouch *touch, uint32_t *phaseOut) {
  switch (touch.phase) {
  case UITouchPhaseBegan:
    *phaseOut = 0;
    return YES;
  case UITouchPhaseMoved:
    *phaseOut = 1;
    return YES;
  case UITouchPhaseEnded:
    *phaseOut = 2;
    return YES;
  case UITouchPhaseCancelled:
    *phaseOut = 3;
    return YES;
  case UITouchPhaseStationary:
    *phaseOut = 1;
    return YES;
  default:
    return NO;
  }
}

static NSString *OxideTouchDebugLogPath(void) {
  NSArray<NSString *> *documents =
      NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask,
                                          YES);
  NSString *base = documents.firstObject;
  if (base.length == 0) {
    base = NSTemporaryDirectory();
  }
  return [base stringByAppendingPathComponent:@"oxide-touch.log"];
}

static BOOL OxideTouchFileLogShouldInclude(NSString *line) {
  if (line.length == 0) {
    return NO;
  }
  NSArray<NSString *> *needles = @[
    @"touch", @"Touch", @"recognizer", @"gesture", @"pointer", @"hover",
    @"pinch", @"pan", @"rust "
  ];
  for (NSString *needle in needles) {
    if ([line rangeOfString:needle].location != NSNotFound) {
      return YES;
    }
  }
  return NO;
}

static void OxideTouchFileLog(NSString *line) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  static dispatch_queue_t queue;
  static NSString *path;
  dispatch_once(&onceToken, ^{
    NSString *value = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_TOUCH_LOG"];
    enabled = value != nil && value.intValue != 0;
    if (!enabled) {
      return;
    }
    queue = dispatch_queue_create("com.oxide.touch-log-file",
                                  DISPATCH_QUEUE_SERIAL);
    path = [OxideTouchDebugLogPath() copy];
    [[NSFileManager defaultManager] removeItemAtPath:path error:nil];
    [[NSFileManager defaultManager] createFileAtPath:path contents:nil
                                          attributes:nil];
    NSString *header =
        [NSString stringWithFormat:@"oxide touch log path=%@\n", path];
    NSData *data = [header dataUsingEncoding:NSUTF8StringEncoding];
    NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
    [handle writeData:data];
    [handle closeFile];
  });
  if (!enabled || path.length == 0 ||
      !OxideTouchFileLogShouldInclude(line)) {
    return;
  }
  NSString *copy = [line copy];
  dispatch_async(queue, ^{
    @autoreleasepool {
      NSString *entry = [NSString
          stringWithFormat:@"%.6f %@\n", CACurrentMediaTime(), copy];
      NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
      if (data.length == 0) {
        return;
      }
      NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
      if (handle == nil) {
        return;
      }
      [handle seekToEndOfFile];
      [handle writeData:data];
      [handle closeFile];
    }
  });
}

static NSString *OxideGestureStateName(UIGestureRecognizerState state) {
  switch (state) {
  case UIGestureRecognizerStatePossible:
    return @"possible";
  case UIGestureRecognizerStateBegan:
    return @"began";
  case UIGestureRecognizerStateChanged:
    return @"changed";
  case UIGestureRecognizerStateEnded:
    return @"ended";
  case UIGestureRecognizerStateCancelled:
    return @"cancelled";
  case UIGestureRecognizerStateFailed:
    return @"failed";
  default:
    return @"recognized";
  }
}

static NSString *OxideTouchPhaseName(UITouchPhase phase) {
  switch (phase) {
  case UITouchPhaseBegan:
    return @"began";
  case UITouchPhaseMoved:
    return @"moved";
  case UITouchPhaseStationary:
    return @"stationary";
  case UITouchPhaseEnded:
    return @"ended";
  case UITouchPhaseCancelled:
    return @"cancelled";
  default:
    break;
  }
  if (@available(iOS 13.4, *)) {
    if (phase == UITouchPhaseRegionEntered) {
      return @"regionEntered";
    }
    if (phase == UITouchPhaseRegionMoved) {
      return @"regionMoved";
    }
    if (phase == UITouchPhaseRegionExited) {
      return @"regionExited";
    }
  }
  return @"unknown";
}

static NSString *OxideTouchTypeName(UITouchType type) {
  switch (type) {
  case UITouchTypeDirect:
    return @"direct";
  case UITouchTypeIndirect:
    return @"indirect";
  case UITouchTypePencil:
    return @"pencil";
  default:
    break;
  }
  if (@available(iOS 13.4, *)) {
    if (type == UITouchTypeIndirectPointer) {
      return @"indirectPointer";
    }
  }
  return @"unknown";
}

static NSString *OxideEventTypeName(UIEventType type) {
  switch (type) {
  case UIEventTypeTouches:
    return @"touches";
  case UIEventTypeMotion:
    return @"motion";
  case UIEventTypeRemoteControl:
    return @"remoteControl";
  case UIEventTypePresses:
    return @"presses";
  default:
    break;
  }
  if (@available(iOS 13.4, *)) {
    if (type == UIEventTypeScroll) {
      return @"scroll";
    }
    if (type == UIEventTypeHover) {
      return @"hover";
    }
    if (type == UIEventTypeTransform) {
      return @"transform";
    }
  }
  return @"unknown";
}

static UIEventButtonMask OxideEventButtonMask(UIEvent *event) {
  if (event == nil) {
    return 0;
  }
  if (@available(iOS 13.4, *)) {
    return event.buttonMask;
  }
  return 0;
}

static UIEventButtonMask OxideRecognizerButtonMask(UIGestureRecognizer *rec) {
  if (rec == nil) {
    return 0;
  }
  if (@available(iOS 13.4, *)) {
    return rec.buttonMask;
  }
  return 0;
}

static NSString *OxideTouchSummary(UITouch *touch, UIView *view) {
  if (touch == nil) {
    return @"nil";
  }
  CGPoint point = [touch locationInView:view];
  CGPoint previous = [touch previousLocationInView:view];
  return [NSString
      stringWithFormat:
          @"phase=%@ type=%@ p=(%.1f,%.1f) prev=(%.1f,%.1f) taps=%lu",
          OxideTouchPhaseName(touch.phase), OxideTouchTypeName(touch.type),
          point.x, point.y, previous.x, previous.y,
          (unsigned long)touch.tapCount];
}

static NSString *OxideTouchCollectionSummary(id<NSFastEnumeration> touches,
                                             UIView *view) {
  NSMutableArray<NSString *> *items = [NSMutableArray array];
  for (UITouch *touch in touches) {
    [items addObject:OxideTouchSummary(touch, view)];
  }
  return [items componentsJoinedByString:@" | "];
}

static NSString *OxideEventSummary(UIEvent *event, UIView *view) {
  if (event == nil) {
    return @"nil";
  }
  NSString *touches = @"";
  NSUInteger touchCount = 0;
  if (event.type == UIEventTypeTouches && event.allTouches.count > 0) {
    touchCount = event.allTouches.count;
    touches = OxideTouchCollectionSummary(event.allTouches, view);
  }
  return [NSString
      stringWithFormat:@"type=%@ raw=%ld buttonMask=%ld modifiers=%lu "
                       @"allTouches=%lu touches=[%@]",
                       OxideEventTypeName(event.type), (long)event.type,
                       (long)OxideEventButtonMask(event),
                       (unsigned long)event.modifierFlags,
                       (unsigned long)touchCount, touches];
}

static NSString *BoolYesNo(BOOL value) { return value ? @"yes" : @"no"; }

static NSString *CameraMatrixName(uint8_t code) {
  switch (code) {
  case 1:
    return @"601";
  case 2:
    return @"2020";
  default:
    return @"709";
  }
}

static NSString *CameraRangeName(uint8_t code) {
  return (code == 0) ? @"full" : @"video";
}

static inline uint64_t ts_now_ns(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static int DeviceMaxFPS(void) {
  CADisplayLink *dl =
      [CADisplayLink displayLinkWithTarget:[NSNull null]
                                  selector:@selector(description)];
  int max = 60;
  if (@available(iOS 15.0, *)) {
    max = (int)dl.preferredFrameRateRange.maximum;
  } else if (@available(iOS 10.3, *)) {
    max = (int)dl.preferredFramesPerSecond;
  }
  [dl invalidate];
  return max > 0 ? max : 60;
}

static int CurrentTargetFPS(void) {
  return oxide_host_is_reduce_motion() ? 60 : DeviceMaxFPS();
}

static void dispatch_on_main(void (^block)(void)) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_async(dispatch_get_main_queue(), block);
  }
}

#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA
static void dispatch_sync_on_main(void (^block)(void)) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}
#endif

static NSString *StringFromUtf8(const char *utf8, size_t len) {
  if (!utf8 || len == 0) {
    return @"";
  }
  return [[NSString alloc] initWithBytes:utf8
                                  length:len
                                encoding:NSUTF8StringEncoding]
             ?: @"";
}

static UIWindow *ResolveWindow(UIView *view) {
  if (view.window) {
    return view.window;
  }
  if (@available(iOS 13.0, *)) {
    NSSet<UIScene *> *scenes = UIApplication.sharedApplication.connectedScenes;
    for (UIScene *scene in scenes) {
      if (![scene isKindOfClass:[UIWindowScene class]]) {
        continue;
      }
      UIWindowScene *ws = (UIWindowScene *)scene;
      for (UIWindow *w in ws.windows) {
        if (w.isKeyWindow) {
          return w;
        }
      }
    }
    for (UIScene *scene in scenes) {
      if (![scene isKindOfClass:[UIWindowScene class]]) {
        continue;
      }
      UIWindowScene *ws = (UIWindowScene *)scene;
      if (ws.windows.count > 0) {
        return ws.windows.firstObject;
      }
    }
  }
  id<UIApplicationDelegate> delegate = UIApplication.sharedApplication.delegate;
  if ([delegate respondsToSelector:@selector(window)]) {
    UIWindow *dw = delegate.window;
    if (dw) {
      return dw;
    }
  }
  return nil;
}

static void EmitWindowMetricsForView(UIView *view) {
  if (!view) {
    return;
  }
  UIWindow *window = ResolveWindow(view);
  if (!window) {
    return;
  }
  CGFloat scale = window.screen.nativeScale;
  CGSize size = view.bounds.size;
  UIEdgeInsets safe = UIEdgeInsetsZero;
  if (@available(iOS 11.0, *)) {
    safe = window.safeAreaInsets;
  }
  OXLOG(@"EmitWindowMetricsForView size=(%.1f,%.1f) scale=%.2f "
        @"safe=(%.1f,%.1f,%.1f,%.1f)",
        size.width, size.height, scale, safe.left, safe.top, safe.right,
        safe.bottom);
  oxide_host_emit_window_resized(
      (float)size.width, (float)size.height, (float)scale, (float)safe.left,
      (float)safe.top, (float)safe.right, (float)safe.bottom);
}

static CGFloat ResolveViewScale(UIView *view) {
  if (view.window) {
    CGFloat scale = view.window.screen.nativeScale;
    if (scale > 0.0) {
      return scale;
    }
  }
  CGFloat trait_scale = view.traitCollection.displayScale;
  if (trait_scale > 0.0) {
    return trait_scale;
  }
  CGFloat fallback_scale = view.contentScaleFactor;
  return fallback_scale > 0.0 ? fallback_scale : 1.0;
}

static void EnsureHostInitialized(UIView *view) {
  gAppDebugPerf.ensure_host_initialized_calls += 1;
  if ((IsRunningUITest() && !ShouldRender() && !IsRunningPerfBenchmarkHost()) ||
      gHostAppReady || !view) {
    return;
  }
  UIWindow *window = ResolveWindow(view);
  if (!window) {
    OXLOG(@"EnsureHostInitialized: no window yet");
    return;
  }
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  CGSize size = layer.drawableSize;
  CGFloat scale = window.screen.nativeScale;
  int32_t rc =
      oxide_host_app_init((uint32_t)lrintf((float)size.width),
                          (uint32_t)lrintf((float)size.height), (float)scale);
  if (rc == 0) {
    gHostAppReady = YES;
    gAppDebugPerf.host_ready_transitions += 1;
    EmitWindowMetricsForView(view);
    uint32_t count = oxide_host_scene_count();
    OXLOG(@"host initialized width=%u height=%u scale=%g scenes=%u",
          (uint32_t)lrintf((float)size.width),
          (uint32_t)lrintf((float)size.height), (double)scale, count);
  } else {
    OXLOG(@"host init failed rc=%d", rc);
  }
}

// Shared clipboard, haptics, and permissions live in platform-ios
// host_services.m.

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
} OxCameraRecordEvent;

enum {
  kOxCamRecordEventCompleted = 0,
  kOxCamRecordEventCancelled = 1,
  kOxCamRecordEventFailed = 2
};

enum {
  kOxCamRecordErrorUnknown = 0,
  kOxCamRecordErrorPermission = 1,
  kOxCamRecordErrorCapability = 2,
  kOxCamRecordErrorNotFound = 3,
  kOxCamRecordErrorBusy = 4,
  kOxCamRecordErrorInvalid = 5,
  kOxCamRecordErrorUnsupported = 6,
  kOxCamRecordErrorIo = 7
};

#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA
static void (*gCameraFrameCallback)(const OxCameraFrame *) = NULL;
static void (*gCameraAudioCallback)(const OxCameraAudio *) = NULL;
static void (*gCameraRecordCallback)(const OxCameraRecordEvent *) = NULL;
static NSString *const kOxCameraRecordErrorDomain = @"com.oxide.camera.record";

static uint64_t timestamp_ms_now(void) {
  struct timespec ts;
  clock_gettime(CLOCK_REALTIME, &ts);
  return (uint64_t)ts.tv_sec * 1000ull + (uint64_t)(ts.tv_nsec / 1000000ull);
}

static uint64_t CMTimeToNs(CMTime t) {
  if (CMTIME_IS_INVALID(t) || t.timescale == 0) {
    return 0;
  }
  CMTime ns = CMTimeConvertScale(t, 1000000000, kCMTimeRoundingMethod_Default);
  if (ns.timescale == 0 || ns.value < 0) {
    return 0;
  }
  return (uint64_t)ns.value;
}

static uint64_t CMTimeSpanToNs(CMTime start, CMTime end) {
  if (CMTIME_IS_INVALID(start) || CMTIME_IS_INVALID(end)) {
    return 0;
  }
  CMTime span = CMTimeSubtract(end, start);
  if (span.timescale == 0 || span.value <= 0) {
    return 0;
  }
  return CMTimeToNs(span);
}

static void EmitCameraRecordEvent(uint32_t kind, NSURL *url,
                                  uint64_t duration_ns, uint64_t size_bytes,
                                  BOOL had_audio, int32_t error_code,
                                  NSString *error_msg) {
  if (!gCameraRecordCallback) {
    return;
  }
  NSData *pathData = nil;
  if (url.path.length) {
    pathData = [url.path dataUsingEncoding:NSUTF8StringEncoding];
  }
  const uint8_t *path_ptr = (const uint8_t *)pathData.bytes;
  size_t path_len = pathData.length;

  NSData *msgData = nil;
  if (error_msg.length) {
    msgData = [error_msg dataUsingEncoding:NSUTF8StringEncoding];
  }
  const uint8_t *msg_ptr = (const uint8_t *)msgData.bytes;
  size_t msg_len = msgData.length;

  OxCameraRecordEvent ev = {.kind = kind,
                            .path_ptr = path_ptr,
                            .path_len = path_len,
                            .duration_ns = duration_ns,
                            .size_bytes = size_bytes,
                            .had_audio = had_audio ? 1 : 0,
                            .error_code = error_code,
                            .error_msg_ptr = msg_ptr,
                            .error_msg_len = msg_len};
  gCameraRecordCallback(&ev);
}

void oxide_host_set_camera_callback(void (*cb)(const OxCameraFrame *)) {
  gCameraFrameCallback = cb;
}

void oxide_host_set_camera_audio_callback(void (*cb)(const OxCameraAudio *)) {
  gCameraAudioCallback = cb;
}

void oxide_host_set_camera_record_callback(
    void (*cb)(const OxCameraRecordEvent *)) {
  gCameraRecordCallback = cb;
}
#endif

// Bluetooth, push, reachability, clipboard, haptics, and permissions now
// live in shared platform-ios shims.

// ===== BLE bridge =====

// ===== Resource loading =====

static uint8_t (*gResourceLoader)(const char *, void **, size_t *) = NULL;

void oxide_host_set_resource_loader(uint8_t (*cb)(const char *, void **,
                                                  size_t *)) {
  gResourceLoader = cb;
}

int32_t oxide_host_resource_read(const char *name, void **out_ptr,
                                 size_t *out_len) {
  if (!out_ptr || !out_len || !name) {
    return 0;
  }
  if (gResourceLoader) {
    uint8_t ok = gResourceLoader(name, out_ptr, out_len);
    if (ok) {
      return 1;
    }
  }
  NSString *resource = StringFromUtf8(name, strlen(name));
  if (resource.length == 0) {
    *out_ptr = NULL;
    *out_len = 0;
    return 0;
  }
  NSString *bundlePath = [[NSBundle mainBundle] resourcePath];
  NSString *fullPath = [bundlePath stringByAppendingPathComponent:resource];
  NSData *data = [NSData dataWithContentsOfFile:fullPath
                                        options:NSDataReadingMappedIfSafe
                                          error:nil];
  if (!data) {
    *out_ptr = NULL;
    *out_len = 0;
    return 0;
  }
  void *buf = malloc(data.length);
  if (!buf) {
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

void oxide_host_ime_show(void) {
  dispatch_on_main(^{
    UIView *view = gMetalView;
    if (view) {
      [view becomeFirstResponder];
    }
  });
}

void oxide_host_ime_hide(void) {
  dispatch_on_main(^{
    UIView *view = gMetalView;
    if (view) {
      [view resignFirstResponder];
    }
  });
}

void nametag_ios_set_ime_content_type(int32_t content_type) {
  dispatch_on_main(^{
    UIView *view = gMetalView;
    if (!view || ![view conformsToProtocol:@protocol(UITextInputTraits)]) {
      return;
    }
    id<UITextInputTraits> traits = (id<UITextInputTraits>)view;

    switch (content_type) {
    case 1:
      traits.keyboardType = UIKeyboardTypeNumberPad;
      traits.autocorrectionType = UITextAutocorrectionTypeNo;
      traits.autocapitalizationType = UITextAutocapitalizationTypeNone;
      if (@available(iOS 12.0, *)) {
        traits.textContentType = UITextContentTypeOneTimeCode;
      } else {
        traits.textContentType = nil;
      }
      break;
    default:
      traits.keyboardType = UIKeyboardTypeDefault;
      traits.textContentType = nil;
      traits.autocorrectionType = UITextAutocorrectionTypeDefault;
      traits.autocapitalizationType = UITextAutocapitalizationTypeSentences;
      break;
    }

    if (view.isFirstResponder) {
      [view reloadInputViews];
    }
  });
}

// ===== Camera implementation (AVFoundation + CVMetalTextureCache) =====
#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA

static const uint32_t kOxCameraPublishedSlotCount = 4;

static inline uint64_t OxPackPublishedFrameState(uint32_t slot,
                                                 uint64_t generation) {
  return ((generation & 0x00FFFFFFFFFFFFFFull) << 8) | (uint64_t)(slot & 0xFFu);
}

static inline uint32_t OxPublishedFrameSlot(uint64_t state) {
  return (uint32_t)(state & 0xFFu);
}

static inline uint64_t OxPublishedFrameGeneration(uint64_t state) {
  return state >> 8;
}

@interface CamFrameSlot : NSObject
@property(nonatomic) CVMetalTextureRef yRef;
@property(nonatomic) CVMetalTextureRef uvRef;
@property(nonatomic, strong) id<MTLTexture> yTex;
@property(nonatomic, strong) id<MTLTexture> uvTex;
@property(nonatomic) int width;
@property(nonatomic) int height;
@property(nonatomic) int bitDepth;
@property(nonatomic) int matrix;
@property(nonatomic) int videoRange;
@property(nonatomic) int colorSpace;
@property(nonatomic) uint64_t generation;
@property(nonatomic) uint64_t timestampNs;
@end

@implementation CamFrameSlot
- (void)dealloc {
  if (_yRef) {
    CFRelease(_yRef);
    _yRef = NULL;
  }
  if (_uvRef) {
    CFRelease(_uvRef);
    _uvRef = NULL;
  }
}
@end

@interface CamCapture : NSObject <AVCaptureVideoDataOutputSampleBufferDelegate,
                                  AVCaptureAudioDataOutputSampleBufferDelegate>
@property(nonatomic, strong) AVCaptureSession *session;
@property(nonatomic, strong) AVCaptureDeviceInput *input;
@property(nonatomic, strong) AVCaptureVideoDataOutput *output;
@property(nonatomic, strong) AVCaptureDeviceInput *audioInput;
@property(nonatomic, strong) AVCaptureAudioDataOutput *audioOutput;
@property(nonatomic) dispatch_queue_t audioQueue;
@property(nonatomic) CVMetalTextureCacheRef cache;
@property(nonatomic) int width;
@property(nonatomic) int height;
@property(nonatomic) int bitDepth;   // 8 or 10
@property(nonatomic) int matrix;     // 0=709,1=601,2=2020
@property(nonatomic) int videoRange; // 0 full, 1 video
@property(nonatomic) int colorSpace; // reserved
@property(nonatomic) uint64_t latestGeneration;
@property(nonatomic, strong) NSMutableArray<CamFrameSlot *> *publishedSlots;
@property(nonatomic) uint32_t publishCursor;
@property(nonatomic) BOOL running;
@property(nonatomic) BOOL wantsAudio;
@property(nonatomic) int desiredFps;
@property(nonatomic) int desiredHeight;
@property(nonatomic) int desiredBitDepth;   // 8 or 10
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

- (BOOL)startRecordingWithPath:(NSString *)path
                     container:(int32_t)container
                  includeAudio:(BOOL)includeAudio
                         error:(NSError **)outError;
- (void)stopRecordingSession;
- (void)cancelRecordingSession;
- (BOOL)isRecording;
@end

@interface CamCapture () {
  _Atomic(uint64_t) _publishedFrameState;
  _Atomic(uint32_t) _slotPins[kOxCameraPublishedSlotCount];
}
- (void)resetRecordingState;
- (void)failRecordingWithError:(NSError *)error;
- (void)ensureRecordSessionStarted:(CMTime)pts;
- (void)appendVideoSample:(CMSampleBufferRef)sampleBuffer;
- (void)appendAudioSample:(CMSampleBufferRef)sampleBuffer;
- (int)reservePublishSlot;
- (BOOL)acquireLatestFrame:(OxCameraAcquiredFrame *)outFrame
               ifNewerThan:(uint64_t)minGenerationExclusive;
@end

@implementation CamCapture
- (instancetype)init {
  self = [super init];
  if (self) {
    _session = [AVCaptureSession new];
    _output = [AVCaptureVideoDataOutput new];
    _audioInput = nil;
    _audioOutput = nil;
    _audioQueue = NULL;
    _wantsAudio = YES;
    _desiredFps = 30;
    _desiredHeight = 1080;
    _desiredBitDepth = 8;
    _desiredColorSpace = 0;
    _desiredPosition = AVCaptureDevicePositionBack;
    _desiredMode = 0;
    _bitDepth = 8;
    _matrix = 0;
    _videoRange = 0;
    _colorSpace = 0;
    _latestGeneration = 0;
    _publishedSlots =
        [NSMutableArray arrayWithCapacity:kOxCameraPublishedSlotCount];
    for (uint32_t idx = 0; idx < kOxCameraPublishedSlotCount; idx++) {
      [_publishedSlots addObject:[CamFrameSlot new]];
      atomic_init(&_slotPins[idx], 0);
    }
    _publishCursor = 0;
    atomic_init(&_publishedFrameState, 0);
    _width = 0;
    _height = 0;
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
    if (gMetalDevice) {
      CVReturn cr = CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL,
                                              gMetalDevice, NULL, &_cache);
      if (cr != kCVReturnSuccess) {
        _cache = NULL;
        OXLOG(@"CamCapture: CVMetalTextureCacheCreate failed %d", cr);
      }
    }
  }
  return self;
}

- (void)dealloc {
  if (_cache) {
    CFRelease(_cache);
    _cache = NULL;
  }
  atomic_store_explicit(&_publishedFrameState, 0, memory_order_release);
}

- (AVCaptureDevice *)selectDevice {
  AVCaptureDevicePosition position = self.desiredPosition;
  AVCaptureDeviceDiscoverySession *discovery = [AVCaptureDeviceDiscoverySession
      discoverySessionWithDeviceTypes:@[
        AVCaptureDeviceTypeBuiltInWideAngleCamera
      ]
                            mediaType:AVMediaTypeVideo
                             position:position];
  for (AVCaptureDevice *dev in discovery.devices) {
    if (dev.position == position) {
      return dev;
    }
  }
  return [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
}

- (AVCaptureDeviceFormat *)selectFormatForDevice:(AVCaptureDevice *)device {
  if (!device) {
    return nil;
  }
  int targetHeight = self.desiredHeight > 0 ? self.desiredHeight : 1080;
  int targetBitDepth = self.desiredBitDepth;
  AVCaptureDeviceFormat *best = device.activeFormat;
  int bestDiff = INT_MAX;
  for (AVCaptureDeviceFormat *format in device.formats) {
    FourCharCode subtype =
        CMFormatDescriptionGetMediaSubType(format.formatDescription);
    BOOL is10Bit =
        (subtype == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange ||
         subtype == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange);
    BOOL is8Bit = (subtype == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange ||
                   subtype == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
    if (targetBitDepth == 10 && !is10Bit) {
      continue;
    }
    if (targetBitDepth == 8 && !(is8Bit || is10Bit)) {
      continue;
    }

    CMVideoDimensions dims =
        CMVideoFormatDescriptionGetDimensions(format.formatDescription);
    if (dims.height <= 0) {
      continue;
    }
    int diff = (int)labs((long)dims.height - (long)targetHeight);
    if (diff < bestDiff) {
      bestDiff = diff;
      best = format;
    }
  }
  return best;
}

- (BOOL)configureSession {
  if (!self.session) {
    return NO;
  }
  [self.session beginConfiguration];
  if (self.input) {
    [self.session removeInput:self.input];
    self.input = nil;
  }
  if (self.audioInput) {
    [self.session removeInput:self.audioInput];
    self.audioInput = nil;
  }
  if (self.output && [[self.session outputs] containsObject:self.output]) {
    [self.session removeOutput:self.output];
  }
  if (self.audioOutput &&
      [[self.session outputs] containsObject:self.audioOutput]) {
    [self.session removeOutput:self.audioOutput];
    self.audioOutput = nil;
  }
  self.audioQueue = NULL;
  self.session.sessionPreset = AVCaptureSessionPresetHigh;

  AVCaptureDevice *device = [self selectDevice];
  if (!device) {
    [self.session commitConfiguration];
    return NO;
  }

  NSError *err = nil;
  self.input = [AVCaptureDeviceInput deviceInputWithDevice:device error:&err];
  if (err || ![self.session canAddInput:self.input]) {
    OXLOG(@"CamCapture: cannot add input: %@", err);
    [self.session commitConfiguration];
    return NO;
  }
  [self.session addInput:self.input];

  NSDictionary *settings = @{
    (NSString *)kCVPixelBufferPixelFormatTypeKey :
        @(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange)
  };
  self.output.videoSettings = settings;
  self.output.alwaysDiscardsLateVideoFrames = YES;
  dispatch_queue_t q =
      dispatch_queue_create("com.oxide.cam", DISPATCH_QUEUE_SERIAL);
  [self.output setSampleBufferDelegate:self queue:q];
  if (![self.session canAddOutput:self.output]) {
    OXLOG(@"CamCapture: cannot add output");
    [self.session commitConfiguration];
    return NO;
  }
  [self.session addOutput:self.output];

  if (self.wantsAudio) {
    AVCaptureDevice *audioDevice =
        [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
    if (audioDevice) {
      NSError *audioErr = nil;
      self.audioInput = [AVCaptureDeviceInput deviceInputWithDevice:audioDevice
                                                              error:&audioErr];
      if (!audioErr && self.audioInput &&
          [self.session canAddInput:self.audioInput]) {
        [self.session addInput:self.audioInput];
      } else {
        if (audioErr) {
          OXLOG(@"CamCapture: cannot add audio input: %@", audioErr);
        }
        self.audioInput = nil;
      }
      self.audioOutput = [AVCaptureAudioDataOutput new];
      self.audioQueue =
          dispatch_queue_create("com.oxide.cam.audio", DISPATCH_QUEUE_SERIAL);
      [self.audioOutput setSampleBufferDelegate:self queue:self.audioQueue];
      if ([self.session canAddOutput:self.audioOutput]) {
        [self.session addOutput:self.audioOutput];
      } else {
        OXLOG(@"CamCapture: cannot add audio output");
        self.audioOutput = nil;
      }
    }
  }

  // Preferred fps if possible
  if ([device lockForConfiguration:&err]) {
    int fps = self.desiredFps > 0 ? self.desiredFps : 30;
    CMTime dur = CMTimeMake(1, fps);
    if ([device
            respondsToSelector:@selector(setActiveVideoMinFrameDuration:)]) {
      device.activeVideoMinFrameDuration = dur;
      device.activeVideoMaxFrameDuration = dur;
    }
    [device unlockForConfiguration];
  }
  [self.session commitConfiguration];
  return YES;
}

- (void)start {
  if (self.running) {
    return;
  }
  AVAuthorizationStatus st =
      [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo];
  if (st == AVAuthorizationStatusNotDetermined) {
    [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo
                             completionHandler:^(BOOL granted) {
                               if (granted) {
                                 dispatch_async(dispatch_get_main_queue(), ^{
                                   [self start];
                                 });
                               }
                             }];
    return;
  }
  if (st != AVAuthorizationStatusAuthorized) {
    OXLOG(@"CamCapture: camera not authorized (status=%ld)", (long)st);
    return;
  }
  if (self.wantsAudio) {
    AVAudioSession *audioSession = [AVAudioSession sharedInstance];
    AVAudioApplicationRecordPermission perm =
        AVAudioApplication.sharedInstance.recordPermission;
    if (perm == AVAudioApplicationRecordPermissionUndetermined) {
      [AVAudioApplication
          requestRecordPermissionWithCompletionHandler:^(BOOL granted) {
            if (granted) {
              dispatch_async(dispatch_get_main_queue(), ^{
                [self start];
              });
            }
          }];
      return;
    }
    NSError *audioErr = nil;
    [audioSession setCategory:AVAudioSessionCategoryPlayAndRecord
                  withOptions:(AVAudioSessionCategoryOptionDefaultToSpeaker |
                               AVAudioSessionCategoryOptionMixWithOthers)
                        error:&audioErr];
    [audioSession setMode:AVAudioSessionModeVideoRecording error:&audioErr];
    [audioSession setActive:YES error:&audioErr];
  }

  if (![self configureSession]) {
    return;
  }
  [self.session startRunning];
  self.running = YES;
}

- (void)stop {
  if (!self.running) {
    return;
  }
  if (self.recording) {
    [self cancelRecordingSession];
  }
  [self.session stopRunning];
  self.running = NO;
  atomic_store_explicit(&_publishedFrameState, 0, memory_order_release);
  self.publishCursor = 0;
  if (self.wantsAudio) {
    NSError *audioErr = nil;
    [[AVAudioSession sharedInstance] setActive:NO error:&audioErr];
  }
}

- (BOOL)isRecording {
  return self.recording || self.writer != nil;
}

- (BOOL)startRecordingWithPath:(NSString *)path
                     container:(int32_t)container
                  includeAudio:(BOOL)includeAudio
                         error:(NSError **)outError {
  if (self.recording || self.writer != nil) {
    if (outError) {
      *outError = [NSError
          errorWithDomain:kOxCameraRecordErrorDomain
                     code:kOxCamRecordErrorBusy
                 userInfo:@{
                   NSLocalizedDescriptionKey : @"Recording already active"
                 }];
    }
    return NO;
  }
  if (!gCameraRecordCallback) {
    if (outError) {
      *outError =
          [NSError errorWithDomain:kOxCameraRecordErrorDomain
                              code:kOxCamRecordErrorCapability
                          userInfo:@{
                            NSLocalizedDescriptionKey :
                                @"No camera recording listener registered"
                          }];
    }
    return NO;
  }

  NSString *extension = (container == 0) ? @"mp4" : @"mov";
  NSString *targetPath = path;
  BOOL temp = NO;
  if (targetPath.length == 0) {
    NSString *fileName = [NSString
        stringWithFormat:@"oxide_%@.%@", [[NSUUID UUID] UUIDString], extension];
    targetPath =
        [NSTemporaryDirectory() stringByAppendingPathComponent:fileName];
    temp = YES;
  }
  NSURL *url = [NSURL fileURLWithPath:targetPath];
  if (!url) {
    if (outError) {
      *outError = [NSError
          errorWithDomain:kOxCameraRecordErrorDomain
                     code:kOxCamRecordErrorInvalid
                 userInfo:@{
                   NSLocalizedDescriptionKey : @"Invalid recording destination"
                 }];
    }
    return NO;
  }
  [[NSFileManager defaultManager] removeItemAtURL:url error:nil];

  AVFileType fileType =
      (container == 0) ? AVFileTypeMPEG4 : AVFileTypeQuickTimeMovie;
  NSError *writerErr = nil;
  AVAssetWriter *writer = [AVAssetWriter assetWriterWithURL:url
                                                   fileType:fileType
                                                      error:&writerErr];
  if (!writer) {
    if (outError) {
      *outError = writerErr
                      ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                             code:kOxCamRecordErrorIo
                                         userInfo:@{
                                           NSLocalizedDescriptionKey :
                                               @"Failed to create asset writer"
                                         }];
    }
    return NO;
  }
  writer.shouldOptimizeForNetworkUse = YES;

  int width = (self.width > 0) ? self.width : 1280;
  int height = (self.height > 0) ? self.height : 720;
  int fps = (self.desiredFps > 0) ? self.desiredFps : 30;
  NSDictionary *compression = @{
    AVVideoAverageBitRateKey : @(width * height * 6),
    AVVideoExpectedSourceFrameRateKey : @(fps),
    AVVideoProfileLevelKey : AVVideoProfileLevelH264HighAutoLevel
  };
  NSDictionary *videoSettings = @{
    AVVideoCodecKey : AVVideoCodecTypeH264,
    AVVideoWidthKey : @(width),
    AVVideoHeightKey : @(height),
    AVVideoCompressionPropertiesKey : compression
  };
  AVAssetWriterInput *videoInput =
      [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeVideo
                                         outputSettings:videoSettings];
  videoInput.expectsMediaDataInRealTime = YES;
  if (![writer canAddInput:videoInput]) {
    if (outError) {
      *outError = [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                      code:kOxCamRecordErrorUnsupported
                                  userInfo:@{
                                    NSLocalizedDescriptionKey :
                                        @"Writer cannot accept video input"
                                  }];
    }
    return NO;
  }
  [writer addInput:videoInput];

  BOOL audioEnabled = includeAudio && self.audioOutput != nil;
  AVAssetWriterInput *audioInput = nil;
  if (audioEnabled) {
    NSDictionary *audioSettings = @{
      AVFormatIDKey : @(kAudioFormatMPEG4AAC),
      AVNumberOfChannelsKey : @(2),
      AVSampleRateKey : @(48000),
      AVEncoderBitRateKey : @(128000)
    };
    audioInput =
        [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeAudio
                                           outputSettings:audioSettings];
    audioInput.expectsMediaDataInRealTime = YES;
    if ([writer canAddInput:audioInput]) {
      [writer addInput:audioInput];
    } else {
      audioEnabled = NO;
      audioInput = nil;
    }
  }

  if (![writer startWriting]) {
    NSError *err = writer.error
                       ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                              code:kOxCamRecordErrorIo
                                          userInfo:@{
                                            NSLocalizedDescriptionKey :
                                                @"Unable to start writer"
                                          }];
    if (outError) {
      *outError = err;
    }
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

- (void)stopRecordingSession {
  if (!self.writer) {
    return;
  }
  if (!self.recording && self.writer.status != AVAssetWriterStatusWriting) {
    return;
  }
  self.recording = NO;
  if (self.writerVideo) {
    [self.writerVideo markAsFinished];
  }
  if (self.recordIncludeAudio && self.writerAudio) {
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
      if (status == AVAssetWriterStatusCompleted) {
        EmitCameraRecordEvent(kOxCamRecordEventCompleted, url, duration, bytes,
                              hadAudio, 0, nil);
      } else {
        NSString *message = error.localizedDescription ?: @"Recording failed";
        EmitCameraRecordEvent(kOxCamRecordEventFailed, url, duration, bytes,
                              hadAudio, kOxCamRecordErrorIo, message);
        if (temp && url) {
          [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
        }
      }
      [self resetRecordingState];
    });
  }];
}

- (void)cancelRecordingSession {
  if (!self.writer) {
    return;
  }
  NSURL *url = self.recordURL;
  BOOL temp = self.recordTemporary;
  BOOL hadAudio = self.recordIncludeAudio;
  self.recording = NO;
  [self.writer cancelWriting];
  [self resetRecordingState];
  if (temp && url) {
    [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
  }
  EmitCameraRecordEvent(kOxCamRecordEventCancelled, url, 0, 0, hadAudio, 0,
                        nil);
}

- (void)resetRecordingState {
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

- (void)failRecordingWithError:(NSError *)error {
  if (!self.writer) {
    return;
  }
  NSURL *url = self.recordURL;
  BOOL temp = self.recordTemporary;
  BOOL hadAudio = self.recordIncludeAudio;
  [self.writer cancelWriting];
  [self resetRecordingState];
  if (temp && url) {
    [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
  }
  NSString *message = error.localizedDescription ?: @"Recording error";
  EmitCameraRecordEvent(kOxCamRecordEventFailed, url, 0, 0, hadAudio,
                        kOxCamRecordErrorIo, message);
}

- (void)ensureRecordSessionStarted:(CMTime)pts {
  if (self.recordWriterStarted) {
    return;
  }
  if (!self.writer) {
    return;
  }
  if (!CMTIME_IS_VALID(pts)) {
    return;
  }
  [self.writer startSessionAtSourceTime:pts];
  self.recordWriterStarted = YES;
  self.recordStartPTS = pts;
  self.recordLastPTS = pts;
  self.recordDurationNs = 0;
}

- (void)appendVideoSample:(CMSampleBufferRef)sampleBuffer {
  if (!self.recording || !self.writer || self.writerVideo == nil) {
    return;
  }
  if (self.writer.status == AVAssetWriterStatusFailed) {
    [self failRecordingWithError:
              self.writer.error
                  ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                         code:kOxCamRecordErrorIo
                                     userInfo:nil]];
    return;
  }
  CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
  if (!self.recordWriterStarted) {
    [self ensureRecordSessionStarted:pts];
  }
  if (!self.recordWriterStarted) {
    return;
  }
  if (!self.writerVideo.isReadyForMoreMediaData) {
    return;
  }
  if (![self.writerVideo appendSampleBuffer:sampleBuffer]) {
    [self failRecordingWithError:
              self.writer.error
                  ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                         code:kOxCamRecordErrorIo
                                     userInfo:nil]];
    return;
  }
  size_t bytes = (size_t)CMSampleBufferGetTotalSampleSize(sampleBuffer);
  self.recordBytes += bytes;
  self.recordLastPTS = pts;
  uint64_t span = CMTimeSpanToNs(self.recordStartPTS, pts);
  if (span > self.recordDurationNs) {
    self.recordDurationNs = span;
  }
}

- (void)appendAudioSample:(CMSampleBufferRef)sampleBuffer {
  if (!self.recording || !self.recordIncludeAudio || !self.writer ||
      self.writerAudio == nil) {
    return;
  }
  if (self.writer.status == AVAssetWriterStatusFailed) {
    [self failRecordingWithError:
              self.writer.error
                  ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                         code:kOxCamRecordErrorIo
                                     userInfo:nil]];
    return;
  }
  CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
  if (!self.recordWriterStarted) {
    [self ensureRecordSessionStarted:pts];
  }
  if (!self.recordWriterStarted) {
    return;
  }
  if (!self.writerAudio.isReadyForMoreMediaData) {
    return;
  }
  if (![self.writerAudio appendSampleBuffer:sampleBuffer]) {
    [self failRecordingWithError:
              self.writer.error
                  ?: [NSError errorWithDomain:kOxCameraRecordErrorDomain
                                         code:kOxCamRecordErrorIo
                                     userInfo:nil]];
    return;
  }
  size_t bytes = (size_t)CMSampleBufferGetTotalSampleSize(sampleBuffer);
  self.recordBytes += bytes;
  uint64_t span = CMTimeSpanToNs(self.recordStartPTS, pts);
  if (span > self.recordDurationNs) {
    self.recordDurationNs = span;
  }
}

- (int)reservePublishSlot {
  uint32_t start = self.publishCursor;
  for (uint32_t offset = 0; offset < kOxCameraPublishedSlotCount; offset++) {
    uint32_t slot = (start + offset) % kOxCameraPublishedSlotCount;
    if (atomic_load_explicit(&_slotPins[slot], memory_order_acquire) == 0) {
      self.publishCursor = (slot + 1) % kOxCameraPublishedSlotCount;
      return (int)slot;
    }
  }
  return -1;
}

- (void)captureOutput:(AVCaptureOutput *)output
    didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
           fromConnection:(AVCaptureConnection *)connection {
  (void)connection;
  if (self.audioOutput && output == self.audioOutput) {
    BOOL wantsCallback = (gCameraAudioCallback != NULL);
    BOOL wantsRecording =
        (self.recordIncludeAudio && (self.recording || self.writer != nil));
    if (!wantsCallback && !wantsRecording) {
      return;
    }
    CMFormatDescriptionRef fmt =
        CMSampleBufferGetFormatDescription(sampleBuffer);
    const AudioStreamBasicDescription *asbd =
        fmt ? CMAudioFormatDescriptionGetStreamBasicDescription(fmt) : NULL;
    if (!asbd) {
      return;
    }
    uint32_t channels = (uint32_t)asbd->mChannelsPerFrame;
    uint32_t sampleRate = (uint32_t)asbd->mSampleRate;
    if (channels == 0 || sampleRate == 0) {
      return;
    }
    CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sampleBuffer);
    if (!block) {
      return;
    }
    size_t length = CMBlockBufferGetDataLength(block);
    if (length == 0) {
      return;
    }
    int16_t *buffer = (int16_t *)malloc(length);
    if (!buffer) {
      return;
    }
    if (CMBlockBufferCopyDataBytes(block, 0, length, buffer) !=
        kCMBlockBufferNoErr) {
      free(buffer);
      return;
    }
    uint64_t timestamp_ns = 0;
    CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
    if (pts.timescale != 0) {
      CMTime nsTime =
          CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default);
      if (nsTime.timescale != 0) {
        timestamp_ns = (uint64_t)nsTime.value;
      }
    }
    [self appendAudioSample:sampleBuffer];
    if (wantsCallback) {
      int16_t *buffer = (int16_t *)malloc(length);
      if (!buffer) {
        return;
      }
      if (CMBlockBufferCopyDataBytes(block, 0, length, buffer) !=
          kCMBlockBufferNoErr) {
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
  if (output != self.output) {
    return;
  }
  CVImageBufferRef pb = CMSampleBufferGetImageBuffer(sampleBuffer);
  if (!pb || !self.cache) {
    return;
  }
  size_t wY = CVPixelBufferGetWidthOfPlane(pb, 0);
  size_t hY = CVPixelBufferGetHeightOfPlane(pb, 0);
  OSType fmt = CVPixelBufferGetPixelFormatType(pb);
  [self appendVideoSample:sampleBuffer];
  int bd = (fmt == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange ||
            fmt == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange)
               ? 10
               : 8;
  int vr = (fmt == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange ||
            fmt == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange)
               ? 1
               : 0;
  int mx = 0; // default 709
  if (@available(iOS 11.0, *)) {
    CFTypeRef val =
        CVBufferCopyAttachment(pb, kCVImageBufferYCbCrMatrixKey, NULL);
    if (val && CFGetTypeID(val) == CFStringGetTypeID()) {
      CFStringRef m = (CFStringRef)val;
      if (CFEqual(m, kCVImageBufferYCbCrMatrix_ITU_R_601_4))
        mx = 1;
      else if (CFEqual(m, kCVImageBufferYCbCrMatrix_ITU_R_2020))
        mx = 2;
      else
        mx = 0;
    }
    if (val) {
      CFRelease(val);
    }
  }
  CVMetalTextureRef yref = NULL;
  CVMetalTextureRef uvref = NULL;
  MTLPixelFormat yfmt =
      (bd == 10) ? MTLPixelFormatR16Unorm : MTLPixelFormatR8Unorm;
  MTLPixelFormat uvfmt =
      (bd == 10) ? MTLPixelFormatRG16Unorm : MTLPixelFormatRG8Unorm;
  CVReturn r0 = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, self.cache, pb, NULL, yfmt, wY, hY, 0, &yref);
  size_t wUV = CVPixelBufferGetWidthOfPlane(pb, 1);
  size_t hUV = CVPixelBufferGetHeightOfPlane(pb, 1);
  CVReturn r1 = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, self.cache, pb, NULL, uvfmt, wUV, hUV, 1, &uvref);
  if (r0 != kCVReturnSuccess || r1 != kCVReturnSuccess) {
    if (yref)
      CFRelease(yref);
    if (uvref)
      CFRelease(uvref);
    return;
  }
  id<MTLTexture> yTex = CVMetalTextureGetTexture(yref);
  id<MTLTexture> uvTex = CVMetalTextureGetTexture(uvref);
  if (!yTex || !uvTex) {
    if (yref)
      CFRelease(yref);
    if (uvref)
      CFRelease(uvref);
    return;
  }
  uint64_t timestamp_ns = 0;
  CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
  if (pts.timescale != 0) {
    CMTime nsTime =
        CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default);
    if (nsTime.timescale != 0) {
      timestamp_ns = (uint64_t)nsTime.value;
    }
  }
  BOOL needsCpuFrameDelivery = (gCameraFrameCallback != NULL);
  if (needsCpuFrameDelivery) {
    CVPixelBufferLockBaseAddress(pb, kCVPixelBufferLock_ReadOnly);
    const uint8_t *yPlane = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
    const uint8_t *uvPlane = CVPixelBufferGetBaseAddressOfPlane(pb, 1);
    size_t strideY = CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
    size_t strideUV = CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
    size_t lenY = strideY * hY;
    size_t lenUV = strideUV * hUV;
    if (yPlane && uvPlane) {
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
    CVPixelBufferUnlockBaseAddress(pb, kCVPixelBufferLock_ReadOnly);
  }

  int slotIndex = [self reservePublishSlot];
  if (slotIndex < 0) {
    if (yref) {
      CFRelease(yref);
    }
    if (uvref) {
      CFRelease(uvref);
    }
    return;
  }

  CamFrameSlot *slot = self.publishedSlots[(NSUInteger)slotIndex];
  if (slot.yRef) {
    CFRelease(slot.yRef);
    slot.yRef = NULL;
  }
  if (slot.uvRef) {
    CFRelease(slot.uvRef);
    slot.uvRef = NULL;
  }
  slot.yRef = yref;
  slot.uvRef = uvref;
  slot.yTex = yTex;
  slot.uvTex = uvTex;
  slot.width = (int)wY;
  slot.height = (int)hY;
  slot.bitDepth = bd;
  slot.matrix = mx;
  slot.videoRange = vr;
  slot.colorSpace = 0;
  self.latestGeneration = self.latestGeneration + 1;
  slot.generation = self.latestGeneration;
  slot.timestampNs = timestamp_ns;
  self.width = (int)wY;
  self.height = (int)hY;
  self.bitDepth = bd;
  self.matrix = mx;
  self.videoRange = vr;
  self.colorSpace = 0;
  atomic_store_explicit(
      &_publishedFrameState,
      OxPackPublishedFrameState((uint32_t)slotIndex, slot.generation),
      memory_order_release);
}

// Thread-safe snapshot of latest textures/metadata for the renderer fast path.
- (BOOL)acquireLatestFrame:(OxCameraAcquiredFrame *)outFrame
               ifNewerThan:(uint64_t)minGenerationExclusive {
  if (outFrame == NULL) {
    return NO;
  }
  while (YES) {
    uint64_t publishedState =
        atomic_load_explicit(&_publishedFrameState, memory_order_acquire);
    if (publishedState == 0) {
      return NO;
    }
    uint64_t generation = OxPublishedFrameGeneration(publishedState);
    if (generation == 0 || generation <= minGenerationExclusive) {
      return NO;
    }
    uint32_t slotIndex = OxPublishedFrameSlot(publishedState);
    if (slotIndex >= kOxCameraPublishedSlotCount) {
      return NO;
    }
    atomic_fetch_add_explicit(&_slotPins[slotIndex], 1, memory_order_acq_rel);
    CamFrameSlot *slot = self.publishedSlots[(NSUInteger)slotIndex];
    if (slot.generation != generation) {
      atomic_fetch_sub_explicit(&_slotPins[slotIndex], 1, memory_order_acq_rel);
      continue;
    }
    if (!slot.yTex || !slot.uvTex || slot.width <= 0 || slot.height <= 0) {
      atomic_fetch_sub_explicit(&_slotPins[slotIndex], 1, memory_order_acq_rel);
      return NO;
    }
    outFrame->y_tex = (__bridge void *)slot.yTex;
    outFrame->uv_tex = (__bridge void *)slot.uvTex;
    outFrame->width = (int32_t)slot.width;
    outFrame->height = (int32_t)slot.height;
    outFrame->bit_depth = (int32_t)slot.bitDepth;
    outFrame->matrix = (int32_t)slot.matrix;
    outFrame->video_range = (int32_t)slot.videoRange;
    outFrame->color_space = (int32_t)slot.colorSpace;
    outFrame->slot = slotIndex;
    outFrame->generation = generation;
    outFrame->timestamp_ns = slot.timestampNs;
    return YES;
  }
}

- (void)restartIfRunning {
  if (!self.running) {
    return;
  }
  [self stop];
  [self start];
}
@end

static CamCapture *gCam = nil;
static CamCapture *EnsureCam(void) {
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    gCam = [CamCapture new];
  });
  return gCam;
}

int32_t oxide_cam_start_default(void) {
  __block int32_t rc = 0;
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.wantsAudio = YES;
    [c start];
    rc = c.running ? 0 : -1;
  });
  return rc;
}

int32_t oxide_cam_start_default_preview_only(void) {
  __block int32_t rc = 0;
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.wantsAudio = NO;
    [c start];
    rc = c.running ? 0 : -1;
  });
  return rc;
}

void oxide_cam_stop(void) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    [c stop];
  });
}

int32_t oxide_cam_set_fps(int32_t fps) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.desiredFps = (int)MAX(fps, 1);
    [c restartIfRunning];
  });
  return 0;
}

int32_t oxide_cam_set_resolution_height(int32_t h) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.desiredHeight = (int)MAX(h, 0);
    [c restartIfRunning];
  });
  return 0;
}

int32_t oxide_cam_set_bit_depth(int32_t bits) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.desiredBitDepth = (int)((bits >= 10) ? 10 : 8);
    [c restartIfRunning];
  });
  return 0;
}

int32_t oxide_cam_set_color_space(int32_t id) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.desiredColorSpace = (int)id;
    [c restartIfRunning];
  });
  return 0;
}

int32_t oxide_cam_set_position(int32_t pos) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.desiredPosition =
        (pos != 0) ? AVCaptureDevicePositionFront : AVCaptureDevicePositionBack;
    [c restartIfRunning];
  });
  return 0;
}

int32_t oxide_cam_set_mode(int32_t mode) {
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    c.desiredMode = mode;
    switch (mode) {
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

int32_t oxide_cam_record_start(const uint8_t *path_ptr, size_t path_len,
                               int32_t container, uint8_t include_audio) {
  __block int32_t rc = 0;
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    NSString *path = nil;
    if (path_ptr && path_len > 0) {
      path = [[NSString alloc] initWithBytes:path_ptr
                                      length:path_len
                                    encoding:NSUTF8StringEncoding];
    }
    NSError *err = nil;
    BOOL ok = [c startRecordingWithPath:path
                              container:container
                           includeAudio:(include_audio != 0)
                                  error:&err];
    if (!ok) {
      rc = -1;
      if (err) {
        OXLOG(@"CamCapture: start recording failed: %@",
              err.localizedDescription);
      }
    }
  });
  return rc;
}

int32_t oxide_cam_record_stop(void) {
  __block int32_t rc = 0;
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    if (![c isRecording]) {
      rc = -1;
      return;
    }
    [c stopRecordingSession];
  });
  return rc;
}

int32_t oxide_cam_record_cancel(void) {
  __block int32_t rc = 0;
  dispatch_sync_on_main(^{
    CamCapture *c = EnsureCam();
    if (![c isRecording]) {
      if (!c.writer) {
        rc = -1;
        return;
      }
    }
    [c cancelRecordingSession];
  });
  return rc;
}

int32_t oxide_cam_get_latest(void **y_tex, void **uv_tex, int32_t *w,
                             int32_t *h) {
  int32_t bd, mx, vr, cs;
  return oxide_cam_get_latest_ex(y_tex, uv_tex, w, h, &bd, &mx, &vr, &cs);
}

int32_t oxide_cam_acquire_latest_frame_ex(uint64_t min_generation_exclusive,
                                          OxCameraAcquiredFrame *out_frame) {
  if (out_frame == NULL) {
    return 0;
  }
  memset(out_frame, 0, sizeof(*out_frame));
  CamCapture *c = EnsureCam();
  return [c acquireLatestFrame:out_frame ifNewerThan:min_generation_exclusive]
             ? 1
             : 0;
}

uint64_t oxide_cam_peek_latest_generation(void) {
  CamCapture *c = EnsureCam();
  return OxPublishedFrameGeneration(
      atomic_load_explicit(&c->_publishedFrameState, memory_order_acquire));
}

uint64_t oxide_cam_peek_latest_timestamp_ns(void) {
  CamCapture *c = EnsureCam();
  uint64_t state =
      atomic_load_explicit(&c->_publishedFrameState, memory_order_acquire);
  if (state == 0) {
    return 0;
  }
  uint32_t slotIndex = OxPublishedFrameSlot(state);
  uint64_t generation = OxPublishedFrameGeneration(state);
  if (generation == 0 || slotIndex >= kOxCameraPublishedSlotCount ||
      slotIndex >= c.publishedSlots.count) {
    return 0;
  }
  CamFrameSlot *publishedSlot = c.publishedSlots[(NSUInteger)slotIndex];
  if (publishedSlot.generation != generation) {
    return 0;
  }
  return publishedSlot.timestampNs;
}

void oxide_cam_release_acquired(uint32_t slot, uint64_t generation) {
  if (slot >= kOxCameraPublishedSlotCount || generation == 0) {
    return;
  }
  CamCapture *c = EnsureCam();
  if (slot >= c.publishedSlots.count) {
    return;
  }
  CamFrameSlot *publishedSlot = c.publishedSlots[(NSUInteger)slot];
  if (publishedSlot.generation != generation) {
    return;
  }
  atomic_fetch_sub_explicit(&c->_slotPins[slot], 1, memory_order_acq_rel);
}

int32_t oxide_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w,
                                int32_t *h, int32_t *bitdepth, int32_t *matrix,
                                int32_t *video_range, int32_t *colorspace) {
  OxCameraAcquiredFrame acquired = {0};
  if (!oxide_cam_acquire_latest_frame_ex(0, &acquired)) {
    return 0;
  }
  id<MTLTexture> y = (__bridge id<MTLTexture>)acquired.y_tex;
  id<MTLTexture> uv = (__bridge id<MTLTexture>)acquired.uv_tex;
  if (y_tex)
    *y_tex = y ? (__bridge_retained void *)y : NULL;
  if (uv_tex)
    *uv_tex = uv ? (__bridge_retained void *)uv : NULL;
  if (w)
    *w = acquired.width;
  if (h)
    *h = acquired.height;
  if (bitdepth)
    *bitdepth = acquired.bit_depth;
  if (matrix)
    *matrix = acquired.matrix;
  if (video_range)
    *video_range = acquired.video_range;
  if (colorspace)
    *colorspace = acquired.color_space;
  oxide_cam_release_acquired(acquired.slot, acquired.generation);
  return 1;
}

#endif

#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA
int32_t oxide_host_power_lowpower(void) {
  if (@available(iOS 9.0, *)) {
    return [[NSProcessInfo processInfo] isLowPowerModeEnabled] ? 1 : 0;
  }
  return 0;
}

int32_t oxide_host_thermal_state(void) {
  if (@available(iOS 11.0, *)) {
    NSProcessInfoThermalState st = [NSProcessInfo processInfo].thermalState;
    switch (st) {
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
#endif

// ===== Metal view =====

@interface MetalView
    : UIView <UIKeyInput, UITextInputTraits, UIGestureRecognizerDelegate>
@property(nonatomic) CGPoint lastHover;
@property(nonatomic, strong)
    NSMutableDictionary<NSValue *, NSNumber *> *touchIds;
@property(nonatomic) UIKeyboardType keyboardType;
@property(nonatomic, copy, nullable) UITextContentType textContentType;
@property(nonatomic) UITextAutocorrectionType autocorrectionType;
@property(nonatomic) UITextAutocapitalizationType autocapitalizationType;
- (void)emitTouch:(UITouch *)touch phase:(uint32_t)phase;
@end

@implementation MetalView
+ (Class)layerClass {
  return [CAMetalLayer class];
}

- (void)oxideCommonInit {
  if (self.touchIds != nil) {
    return;
  }
  OXLOG(@"MetalView init");
  self.touchIds = [NSMutableDictionary dictionary];
  self.multipleTouchEnabled = YES;
  self.opaque = YES;
  self.backgroundColor = [UIColor whiteColor];
  self.isAccessibilityElement = YES;
  self.accessibilityIdentifier = @"metalView";
  self.accessibilityLabel = @"Oxide Metal View";
  self.keyboardType = UIKeyboardTypeDefault;
  self.textContentType = nil;
  self.autocorrectionType = UITextAutocorrectionTypeDefault;
  self.autocapitalizationType = UITextAutocapitalizationTypeSentences;

  CAMetalLayer *layer = (CAMetalLayer *)self.layer;
  // Bind a device and align format with renderer (sRGB)
  id<MTLDevice> dev = MTLCreateSystemDefaultDevice();
  gMetalDevice = dev;
  layer.device = dev;
  layer.pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB;
  layer.framebufferOnly = YES;
  layer.presentsWithTransaction = NO;
  if (@available(iOS 13.0, *)) {
    layer.allowsNextDrawableTimeout = NO;
  }
  if (@available(iOS 11.2, *)) {
    layer.maximumDrawableCount = OxidePerfCameraMaximumDrawableCount();
  }
  layer.contentsScale =
      MAX(ResolveViewScale(self) * OxidePerfCameraPreviewSurfaceScale(), 1.0);
  OXLOG(@"Layer setup: device=%p format=%lu framebufferOnly=%d "
        @"contentsScale=%.2f previewScale=%.2f maxDrawable=%lu",
        layer.device, (unsigned long)layer.pixelFormat,
        (int)layer.framebufferOnly, layer.contentsScale,
        OxidePerfCameraPreviewSurfaceScale(),
        (unsigned long)layer.maximumDrawableCount);
  StartMetalCaptureIfEnabled(dev);

  UIPinchGestureRecognizer *pinch =
      [[UIPinchGestureRecognizer alloc] initWithTarget:self
                                                action:@selector(onPinch:)];
  pinch.delaysTouchesBegan = NO;
  pinch.delaysTouchesEnded = NO;
  pinch.cancelsTouchesInView = NO;
  pinch.requiresExclusiveTouchType = NO;
  if (@available(iOS 13.4, *)) {
    pinch.allowedTouchTypes = @[
      @(UITouchTypeDirect), @(UITouchTypeIndirect),
      @(UITouchTypeIndirectPointer)
    ];
  } else {
    pinch.allowedTouchTypes = @[ @(UITouchTypeDirect), @(UITouchTypeIndirect) ];
  }
  pinch.delegate = self;
  [self addGestureRecognizer:pinch];

  UIPanGestureRecognizer *pan =
      [[UIPanGestureRecognizer alloc] initWithTarget:self
                                              action:@selector(onPan:)];
  pan.minimumNumberOfTouches = 1;
  pan.maximumNumberOfTouches = 2;
  pan.delaysTouchesBegan = NO;
  pan.delaysTouchesEnded = NO;
  pan.cancelsTouchesInView = NO;
  pan.requiresExclusiveTouchType = NO;
  if (@available(iOS 13.4, *)) {
    pan.allowedTouchTypes = @[
      @(UITouchTypeDirect), @(UITouchTypeIndirect),
      @(UITouchTypeIndirectPointer)
    ];
    pan.allowedScrollTypesMask = UIScrollTypeMaskAll;
  } else {
    pan.allowedTouchTypes = @[ @(UITouchTypeDirect), @(UITouchTypeIndirect) ];
  }
  pan.delegate = self;
  [self addGestureRecognizer:pan];

  UITapGestureRecognizer *doubleTap =
      [[UITapGestureRecognizer alloc] initWithTarget:self
                                              action:@selector(onDoubleTap:)];
  doubleTap.numberOfTapsRequired = 2;
  doubleTap.delaysTouchesBegan = NO;
  doubleTap.delaysTouchesEnded = NO;
  doubleTap.cancelsTouchesInView = NO;
  [self addGestureRecognizer:doubleTap];

  if (@available(iOS 13.4, *)) {
    UIHoverGestureRecognizer *hover =
        [[UIHoverGestureRecognizer alloc] initWithTarget:self
                                                  action:@selector(onHover:)];
    [self addGestureRecognizer:hover];
  }
}

- (BOOL)gestureRecognizer:(UIGestureRecognizer *)gestureRecognizer
    shouldRecognizeSimultaneouslyWithGestureRecognizer:
        (UIGestureRecognizer *)otherGestureRecognizer {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch recognizer simultaneous first=%@/%@ second=%@/%@",
          NSStringFromClass(gestureRecognizer.class),
          OxideGestureStateName(gestureRecognizer.state),
          NSStringFromClass(otherGestureRecognizer.class),
          OxideGestureStateName(otherGestureRecognizer.state));
  }
  return YES;
}

- (BOOL)gestureRecognizer:(UIGestureRecognizer *)gestureRecognizer
       shouldReceiveEvent:(UIEvent *)event API_AVAILABLE(ios(13.4)) {
  if (OxideWindowTouchCaptureEnabled() &&
      event.type == UIEventTypeTouches &&
      OxideEventHasOnlyDirectTouches(event)) {
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"touch recognizer shouldReceiveEvent blocked direct touches "
            @"class=%@ state=%@ all=%lu",
            NSStringFromClass(gestureRecognizer.class),
            OxideGestureStateName(gestureRecognizer.state),
            (unsigned long)event.allTouches.count);
    }
    return NO;
  }
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch recognizer shouldReceiveEvent class=%@ state=%@ event=%@ "
          @"buttonMask=%ld modifiers=%lu all=%lu touches=[%@]",
          NSStringFromClass(gestureRecognizer.class),
          OxideGestureStateName(gestureRecognizer.state),
          OxideEventTypeName(event.type), (long)OxideEventButtonMask(event),
          (unsigned long)event.modifierFlags,
          (unsigned long)event.allTouches.count,
          OxideTouchCollectionSummary(event.allTouches, self));
  }
  return YES;
}

- (BOOL)gestureRecognizer:(UIGestureRecognizer *)gestureRecognizer
       shouldReceiveTouch:(UITouch *)touch {
  if (OxideWindowTouchCaptureEnabled() && OxideTouchIsDirect(touch)) {
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"touch recognizer shouldReceiveTouch blocked direct touch "
            @"class=%@ state=%@ touch=[%@]",
            NSStringFromClass(gestureRecognizer.class),
            OxideGestureStateName(gestureRecognizer.state),
            OxideTouchSummary(touch, self));
    }
    return NO;
  }
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch recognizer shouldReceiveTouch class=%@ state=%@ touch=[%@]",
          NSStringFromClass(gestureRecognizer.class),
          OxideGestureStateName(gestureRecognizer.state),
          OxideTouchSummary(touch, self));
  }
  return YES;
}

- (BOOL)gestureRecognizerShouldBegin:(UIGestureRecognizer *)gestureRecognizer {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch recognizer shouldBegin class=%@ state=%@ touches=%lu "
          @"buttonMask=%ld",
          NSStringFromClass(gestureRecognizer.class),
          OxideGestureStateName(gestureRecognizer.state),
          (unsigned long)gestureRecognizer.numberOfTouches,
          (long)OxideRecognizerButtonMask(gestureRecognizer));
  }
  return YES;
}

- (instancetype)init {
  self = [super init];
  if (self) {
    [self oxideCommonInit];
  }
  return self;
}

- (instancetype)initWithFrame:(CGRect)frame {
  self = [super initWithFrame:frame];
  if (self) {
    [self oxideCommonInit];
  }
  return self;
}

- (instancetype)initWithCoder:(NSCoder *)coder {
  self = [super initWithCoder:coder];
  if (self) {
    [self oxideCommonInit];
  }
  return self;
}

- (BOOL)canBecomeFirstResponder {
  return YES;
}
- (NSString *)accessibilityValue {
  return @"";
}
- (BOOL)hasText {
  return NO;
}
- (void)insertText:(NSString *)text {
  NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
  if (data.length > 0) {
    oxide_host_emit_text_commit(data.bytes, data.length);
  }
}
- (void)deleteBackward {
  const char backspace = '\b';
  oxide_host_emit_text_commit(&backspace, 1);
}

- (void)layoutSubviews {
  [super layoutSubviews];
  CAMetalLayer *layer = (CAMetalLayer *)self.layer;
  CGFloat scale =
      MAX(ResolveViewScale(self) * OxidePerfCameraPreviewSurfaceScale(), 1.0);
  layer.contentsScale = scale;
  layer.drawableSize = CGSizeMake(self.bounds.size.width * scale,
                                  self.bounds.size.height * scale);
  OXLOG(@"layoutSubviews: bounds=(%.1f,%.1f) scale=%.2f previewScale=%.2f "
        @"drawableSize=(%.1f,%.1f)",
        self.bounds.size.width, self.bounds.size.height, scale,
        OxidePerfCameraPreviewSurfaceScale(), layer.drawableSize.width,
        layer.drawableSize.height);
  EmitWindowMetricsForView(self);
}

- (NSNumber *)ensureIdForTouch:(UITouch *)touch {
  NSValue *key = [NSValue valueWithNonretainedObject:touch];
  NSNumber *existing = self.touchIds[key];
  if (existing) {
    return existing;
  }
  static uint64_t nextId = 1;
  NSNumber *newId = @(nextId++);
  self.touchIds[key] = newId;
  return newId;
}

- (void)removeIdForTouch:(UITouch *)touch {
  NSValue *key = [NSValue valueWithNonretainedObject:touch];
  [self.touchIds removeObjectForKey:key];
}

- (void)emitTouch:(UITouch *)touch phase:(uint32_t)phase {
  CGPoint p = [touch locationInView:self];
  CGFloat pressure = 0.0f;
  uint8_t hasP = 0;
  if (@available(iOS 9.0, *)) {
    if (touch.maximumPossibleForce > 0.0f) {
      pressure = touch.force / touch.maximumPossibleForce;
      hasP = 1;
    }
  }
  CGFloat alt = 0.0f, azi = 0.0f;
  uint8_t hasT = 0;
  if (@available(iOS 9.1, *)) {
    alt = touch.altitudeAngle;
    if ([touch respondsToSelector:@selector(azimuthAngleInView:)]) {
      azi = [touch azimuthAngleInView:self];
      hasT = 1;
    }
  }
  uint32_t device = 0;
  if (@available(iOS 9.1, *)) {
    if (touch.type == UITouchTypePencil)
      device = 1;
  }
  NSNumber *idNum = [self ensureIdForTouch:touch];
  uint64_t id = idNum.unsignedLongLongValue;
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch emit generic id=%llu phase=%u touch=[%@]",
          (unsigned long long)id, phase, OxideTouchSummary(touch, self));
  }
  oxide_host_emit_touch(id, phase, p.x, p.y, pressure, hasP, alt, azi, hasT,
                        device, ts_now_ns());
  if (phase == 2 || phase == 3) {
    [self removeIdForTouch:touch];
  }
}

- (void)touchesBegan:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touchesBegan event=%@ buttonMask=%ld touches=%lu all=%lu "
          @"touches=[%@]",
          OxideEventTypeName(event.type), (long)OxideEventButtonMask(event),
          (unsigned long)touches.count, (unsigned long)event.allTouches.count,
          OxideTouchCollectionSummary(touches, self));
  }
  if (OxideWindowTouchCaptureEnabled()) {
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"touchesBegan skipped window touch capture active");
    }
    return;
  }
  for (UITouch *t in touches) {
    [self emitTouch:t phase:0];
  }
}
- (void)touchesMoved:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touchesMoved event=%@ buttonMask=%ld touches=%lu all=%lu "
          @"touches=[%@]",
          OxideEventTypeName(event.type), (long)OxideEventButtonMask(event),
          (unsigned long)touches.count, (unsigned long)event.allTouches.count,
          OxideTouchCollectionSummary(touches, self));
  }
  if (OxideWindowTouchCaptureEnabled()) {
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"touchesMoved skipped window touch capture active");
    }
    return;
  }
  for (UITouch *t in touches) {
    [self emitTouch:t phase:1];
  }
}
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touchesEnded event=%@ buttonMask=%ld touches=%lu all=%lu "
          @"touches=[%@]",
          OxideEventTypeName(event.type), (long)OxideEventButtonMask(event),
          (unsigned long)touches.count, (unsigned long)event.allTouches.count,
          OxideTouchCollectionSummary(touches, self));
  }
  if (OxideWindowTouchCaptureEnabled()) {
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"touchesEnded skipped window touch capture active");
    }
    return;
  }
  for (UITouch *t in touches) {
    [self emitTouch:t phase:2];
  }
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches
               withEvent:(UIEvent *)event {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touchesCancelled event=%@ buttonMask=%ld touches=%lu all=%lu "
          @"touches=[%@]",
          OxideEventTypeName(event.type), (long)OxideEventButtonMask(event),
          (unsigned long)touches.count, (unsigned long)event.allTouches.count,
          OxideTouchCollectionSummary(touches, self));
  }
  if (OxideWindowTouchCaptureEnabled()) {
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"touchesCancelled skipped window touch capture active");
    }
    return;
  }
  for (UITouch *t in touches) {
    [self emitTouch:t phase:3];
  }
}

- (void)onHover:(UIHoverGestureRecognizer *)rec API_AVAILABLE(ios(13.4)) {
  CGPoint p = [rec locationInView:self];
  CGPoint last = self.lastHover;
  self.lastHover = p;
  float dx = p.x - last.x, dy = p.y - last.y;
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch hover state=%@ point=(%.1f,%.1f) delta=(%.1f,%.1f) "
          @"buttonMask=%ld",
          OxideGestureStateName(rec.state), p.x, p.y, dx, dy,
          (long)OxideRecognizerButtonMask(rec));
  }
  oxide_host_emit_pointer(p.x, p.y, dx, dy, 0, 0, ts_now_ns());
}

- (void)onPinch:(UIPinchGestureRecognizer *)rec {
  if (OxideTouchDebugEnabled()) {
    CGPoint c = [rec locationInView:self];
    OXLOG(@"touch recognizer pinch state=%@ scale=%.4f center=(%.1f,%.1f) "
          @"touches=%lu buttonMask=%ld",
          OxideGestureStateName(rec.state), rec.scale, c.x, c.y,
          (unsigned long)rec.numberOfTouches,
          (long)OxideRecognizerButtonMask(rec));
  }
  if (rec.state == UIGestureRecognizerStateBegan ||
      rec.state == UIGestureRecognizerStateChanged) {
    CGPoint c = [rec locationInView:self];
    CGFloat scale = rec.scale;
    if (scale > 0.0f) {
      float delta = log2f(scale);
      if (OxideTouchDebugEnabled()) {
        OXLOG(@"touch recognizer pinch emit center=(%.1f,%.1f) delta=%.4f",
              c.x, c.y, delta);
      }
      oxide_host_emit_pinch(c.x, c.y, delta);
    } else if (OxideTouchDebugEnabled()) {
      OXLOG(@"touch recognizer pinch ignored invalid scale=%.4f", scale);
    }
    rec.scale = 1.0f;
  } else if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch recognizer pinch no-emit state=%@",
          OxideGestureStateName(rec.state));
  }
}

- (void)onPan:(UIPanGestureRecognizer *)rec {
  if (OxideTouchDebugEnabled()) {
    CGPoint p = [rec locationInView:self];
    CGPoint d = [rec translationInView:self];
    CGPoint v = [rec velocityInView:self];
    OXLOG(@"touch recognizer pan state=%@ point=(%.1f,%.1f) delta=(%.1f,%.1f) "
          @"velocity=(%.1f,%.1f) touches=%lu buttonMask=%ld",
          OxideGestureStateName(rec.state), p.x, p.y, d.x, d.y,
          v.x, v.y, (unsigned long)rec.numberOfTouches,
          (long)OxideRecognizerButtonMask(rec));
  }
  if (rec.state == UIGestureRecognizerStateBegan ||
      rec.state == UIGestureRecognizerStateChanged) {
    CGPoint p = [rec locationInView:self];
    CGPoint d = [rec translationInView:self];
    if (isfinite(p.x) && isfinite(p.y) && isfinite(d.x) && isfinite(d.y)) {
      if (OxideTouchDebugEnabled()) {
        OXLOG(@"touch recognizer pan emit point=(%.1f,%.1f) "
              @"delta=(%.1f,%.1f)",
              p.x, p.y, d.x, d.y);
      }
      oxide_host_emit_pan_gesture(p.x, p.y, d.x, d.y, 1);
    } else if (OxideTouchDebugEnabled()) {
      OXLOG(@"touch recognizer pan ignored invalid point/delta");
    }
    [rec setTranslation:CGPointZero inView:self];
  } else if (rec.state == UIGestureRecognizerStateEnded ||
             rec.state == UIGestureRecognizerStateCancelled ||
             rec.state == UIGestureRecognizerStateFailed) {
    CGPoint p = [rec locationInView:self];
    if (isfinite(p.x) && isfinite(p.y)) {
      if (OxideTouchDebugEnabled()) {
        OXLOG(@"touch recognizer pan end point=(%.1f,%.1f)", p.x, p.y);
      }
      oxide_host_emit_pan_gesture(p.x, p.y, 0.0f, 0.0f, 0);
    } else if (OxideTouchDebugEnabled()) {
      OXLOG(@"touch recognizer pan end ignored invalid point");
    }
  } else if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch recognizer pan no-emit state=%@",
          OxideGestureStateName(rec.state));
  }
}

- (void)onDoubleTap:(UITapGestureRecognizer *)rec {
  if (rec.state == UIGestureRecognizerStateRecognized) {
    oxide_host_emit_double_tap();
  }
}

@end

@interface OxideTouchWindow : UIWindow
@end

@implementation OxideTouchWindow
- (void)sendEvent:(UIEvent *)event {
  if (OxideTouchDebugEnabled()) {
    OXLOG(@"touch-debug window sendEvent %@", OxideEventSummary(event, self));
  }
  if (OxideWindowTouchCaptureEnabled() && event.type == UIEventTypeTouches) {
    NSSet<UITouch *> *touches = event.allTouches;
    MetalView *view = [gMetalView isKindOfClass:[MetalView class]]
                          ? (MetalView *)gMetalView
                          : nil;
    if (OxideTouchDebugEnabled()) {
      OXLOG(@"window touch sendEvent type=%@ buttonMask=%ld touches=%lu "
            @"targetMetal=%d touches=[%@]",
            OxideEventTypeName(event.type), (long)OxideEventButtonMask(event),
            (unsigned long)touches.count, view != nil,
            OxideTouchCollectionSummary(touches, view ?: self));
    }
    if (view != nil) {
      for (UITouch *touch in touches) {
        uint32_t phase = 0;
        if (OxideTouchPhaseForUITouch(touch, &phase)) {
          if (OxideTouchDebugEnabled()) {
            OXLOG(@"window touch emit phase=%u touch=[%@]", phase,
                  OxideTouchSummary(touch, view));
          }
          [view emitTouch:touch phase:phase];
        } else if (OxideTouchDebugEnabled()) {
          OXLOG(@"window touch ignored unknown phase touch=[%@]",
                OxideTouchSummary(touch, view));
        }
      }
    }
  }
  [super sendEvent:event];
}
@end

@interface OxidePerfCameraPreviewView : UIView
@property(nonatomic, readonly) AVCaptureVideoPreviewLayer *previewLayer;
@end

@implementation OxidePerfCameraPreviewView
+ (Class)layerClass {
  return [AVCaptureVideoPreviewLayer class];
}

- (AVCaptureVideoPreviewLayer *)previewLayer {
  return (AVCaptureVideoPreviewLayer *)self.layer;
}
@end

@interface RustSceneDelegate
    : UIResponder <UIWindowSceneDelegate, UITextViewDelegate>
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
@property(nonatomic, strong) OxidePerfCameraPreviewView *perfCameraPreviewView;
@property(nonatomic, strong) UILabel *perfBenchmarkStateLabel;
@property(nonatomic, strong) AVCaptureSession *perfBenchmarkAVFoundationSession;
@property(nonatomic) dispatch_queue_t perfBenchmarkAVFoundationQueue;
@property(nonatomic) BOOL perfBenchmarkConfigured;
@property(nonatomic) BOOL perfBenchmarkReady;
@property(nonatomic) BOOL perfBenchmarkRunning;
@property(nonatomic) BOOL perfBenchmarkReadyEmitted;
@property(nonatomic) uint32_t perfBenchmarkCompletedRuns;
@property(nonatomic, copy)
    NSString *perfBenchmarkIterationStartNotificationName;
@property(nonatomic) BOOL hasRealScenes;
- (IBAction)sceneChanged:(UISegmentedControl *)control;
- (IBAction)onOverlaySwitch:(UISwitch *)sw;
- (IBAction)onReduceMotionSwitch:(UISwitch *)sw;
- (void)updateDisplayLinkRange;
- (IBAction)onCamBlur:(UISwitch *)sw;
- (IBAction)onCamGray:(UISwitch *)sw;
- (IBAction)onCamAnim:(UISwitch *)sw;
- (IBAction)onCamSigma:(UISlider *)slider;
- (IBAction)onCamCapture:(UISwitch *)sw;
- (IBAction)onImeFocus:(UIButton *)button;
- (IBAction)onImeBlur:(UIButton *)button;
- (IBAction)onImeCopy:(UIButton *)button;
- (IBAction)onImePaste:(UIButton *)button;
- (IBAction)onImeHaptic:(UIButton *)button;
- (void)updateCameraDrivenDisplayLinkState;
- (void)installCameraDrivenSchedulingCallbackIfNeeded;
- (void)handleActualAppCameraBenchmarkStart;
@end

static void OxideCameraPreviewPublishDidAdvance(uint64_t generation,
                                                uint64_t timestamp_ns,
                                                void *context) {
  (void)generation;
  (void)timestamp_ns;
  (void)context;
  atomic_store_explicit(&gCameraPreviewNeedsPresent, 1, memory_order_release);
  if (!OxidePerfCameraFrameDrivenSchedulingEnabled()) {
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    RustSceneDelegate *delegate = gActiveRustSceneDelegate;
    if (delegate == nil) {
      return;
    }
    [delegate updateCameraDrivenDisplayLinkState];
  });
}

static NSString *const kOxidePerfReadyNotification = @"com.oxide.perf.ready";
static NSString *const kOxidePerfCompleteNotification =
    @"com.oxide.perf.complete";
static NSString *const kOxidePerfBenchmarkStateLabelIdentifier =
    @"perfCameraBenchmarkStateLabel";
static NSString *const kOxidePerfCameraContractSummaryPrefix =
    @"OXIDE_CAMERA_CONTRACT_SUMMARY ";
static NSString *const kOxidePerfAppHostDebugSummaryPrefix =
    @"OXIDE_APP_HOST_DEBUG_SUMMARY ";
static CFStringRef const kOxidePerfStartNotificationCF =
    CFSTR("com.oxide.perf.start");

static NSString *OxidePerfIterationStartNotificationName(uint32_t run) {
  return [NSString stringWithFormat:@"com.oxide.perf.start.%u", run];
}

static NSString *OxidePerfActualAppBootState(void) {
  return [NSString
      stringWithFormat:@"boot ui:%d perf:%d real:%d render:%d case:%@",
                       (int)IsRunningUITest(),
                       (int)IsRunningPerfBenchmarkHost(),
                       (int)OxidePerfCameraRealAppHostEnabled(),
                       (int)ShouldRender(), OxidePerfCaseName()];
}

static int32_t OxideResolveSceneIndexNamed(const char *target_utf8) {
  if (target_utf8 == NULL || target_utf8[0] == '\0') {
    return -1;
  }
  uint32_t count = oxide_host_scene_count();
  for (uint32_t index = 0; index < count; index++) {
    uint32_t needed = oxide_host_scene_name(index, NULL, 0);
    if (needed == 0) {
      continue;
    }
    char *buffer = calloc((size_t)needed, sizeof(char));
    if (buffer == NULL) {
      return -1;
    }
    uint32_t written = oxide_host_scene_name(index, buffer, needed);
    BOOL matched = written > 0 && strcmp(buffer, target_utf8) == 0;
    free(buffer);
    if (matched) {
      return (int32_t)index;
    }
  }
  return -1;
}

static void OxidePerfCameraBenchmarkStartCallback(
    CFNotificationCenterRef center, void *observer, CFStringRef name,
    const void *object, CFDictionaryRef userInfo) {
  (void)center;
  (void)name;
  (void)object;
  (void)userInfo;
  if (observer == NULL) {
    return;
  }
  RustSceneDelegate *delegate = (__bridge RustSceneDelegate *)observer;
  dispatch_async(dispatch_get_main_queue(), ^{
    [delegate handleActualAppCameraBenchmarkStart];
  });
}

@implementation RustSceneDelegate
- (void)updateActualAppCameraBenchmarkIterationStartObserver {
  CFNotificationCenterRef center = CFNotificationCenterGetDarwinNotifyCenter();
  if (self.perfBenchmarkIterationStartNotificationName.length > 0) {
    CFNotificationCenterRemoveObserver(
        center, (__bridge const void *)(self),
        (__bridge CFNotificationName)
            self.perfBenchmarkIterationStartNotificationName,
        NULL);
    self.perfBenchmarkIterationStartNotificationName = nil;
  }
  if (!OxidePerfActualAppCameraBenchmarkEnabled()) {
    return;
  }
  NSString *name = OxidePerfIterationStartNotificationName(
      self.perfBenchmarkCompletedRuns + 1);
  self.perfBenchmarkIterationStartNotificationName = name;
  CFNotificationCenterAddObserver(
      center, (__bridge const void *)(self),
      OxidePerfCameraBenchmarkStartCallback, (__bridge CFStringRef)name, NULL,
      CFNotificationSuspensionBehaviorDeliverImmediately);
}

- (void)updateCameraDrivenDisplayLinkState {
  if (!self.displayLink) {
    return;
  }
  if (!OxidePerfCameraFrameDrivenSchedulingEnabled()) {
    self.displayLink.paused = NO;
    return;
  }
  BOOL shouldRun = atomic_load_explicit(&gCameraPreviewNeedsPresent,
                                        memory_order_acquire) != 0;
  self.displayLink.paused = !shouldRun;
}

- (void)installCameraDrivenSchedulingCallbackIfNeeded {
  gActiveRustSceneDelegate = self;
  if (OxidePerfCameraFrameDrivenSchedulingEnabled()) {
    atomic_store_explicit(&gCameraPreviewNeedsPresent, 1, memory_order_release);
    oxide_cam_set_preview_publish_callback(OxideCameraPreviewPublishDidAdvance,
                                           NULL);
  } else {
    oxide_cam_set_preview_publish_callback(NULL, NULL);
  }
}

- (void)updateActualAppCameraBenchmarkState:(NSString *)state {
  UILabel *label = self.perfBenchmarkStateLabel;
  if (label == nil) {
    return;
  }
  label.text = state ?: @"";
  label.accessibilityLabel = state ?: @"";
  label.accessibilityValue = state ?: @"";
}

- (void)installPerfBenchmarkStateLabelIfNeededInView:(UIView *)parentView {
  if (parentView == nil) {
    return;
  }
  if (self.perfBenchmarkStateLabel != nil) {
    [self updateActualAppCameraBenchmarkState:OxidePerfActualAppBootState()];
    return;
  }
  UILabel *benchmarkLabel = [UILabel new];
  benchmarkLabel.translatesAutoresizingMaskIntoConstraints = NO;
  benchmarkLabel.accessibilityIdentifier =
      kOxidePerfBenchmarkStateLabelIdentifier;
  benchmarkLabel.isAccessibilityElement = YES;
  benchmarkLabel.hidden = NO;
  benchmarkLabel.alpha = 1.0;
  benchmarkLabel.textColor = [UIColor blackColor];
  benchmarkLabel.backgroundColor =
      [[UIColor yellowColor] colorWithAlphaComponent:0.35];
  benchmarkLabel.font = [UIFont systemFontOfSize:12.0];
  benchmarkLabel.numberOfLines = 2;
  [parentView addSubview:benchmarkLabel];
  [NSLayoutConstraint activateConstraints:@[
    [benchmarkLabel.leadingAnchor
        constraintEqualToAnchor:parentView.leadingAnchor
                       constant:8.0],
    [benchmarkLabel.topAnchor
        constraintEqualToAnchor:parentView.safeAreaLayoutGuide.topAnchor
                       constant:8.0],
    [benchmarkLabel.widthAnchor constraintGreaterThanOrEqualToConstant:160.0],
    [benchmarkLabel.heightAnchor constraintGreaterThanOrEqualToConstant:20.0],
  ]];
  self.perfBenchmarkStateLabel = benchmarkLabel;
  [self updateActualAppCameraBenchmarkState:OxidePerfActualAppBootState()];
}

- (void)emitActualAppCameraContractSummary {
  if (OxidePerfActualAppAVFoundationCameraBenchmarkEnabled()) {
    AVCaptureSession *session = self.perfBenchmarkAVFoundationSession;
    AVCaptureDeviceInput *input = session.inputs.firstObject;
    AVCaptureDevice *device = input.device;
    AVCaptureDeviceFormat *format = device.activeFormat;
    CMVideoDimensions dims =
        format != nil
            ? CMVideoFormatDescriptionGetDimensions(format.formatDescription)
            : (CMVideoDimensions){1280, 720};
    Float64 activeFps = 30.0;
    CMTime frameDuration = device.activeVideoMinFrameDuration;
    if (CMTIME_IS_VALID(frameDuration) && frameDuration.value != 0) {
      activeFps = (double)frameDuration.timescale / (double)frameDuration.value;
    }
    NSDictionary *payload = @{
      @"source" : @"avfoundation-preview-layer-real-app",
      @"transport" : @"AVCaptureVideoPreviewLayer",
      @"devicePosition" : @"back",
      @"sessionPreset" : OxidePerfCameraCaptureContractPresetName(),
      @"requestedPixelFormat" : @"420f",
      @"activePixelFormat" : @"420f",
      @"requestedWidth" : @(1280),
      @"requestedHeight" : @(720),
      @"requestedFps" : @(30),
      @"activeWidth" : @((int)dims.width),
      @"activeHeight" : @((int)dims.height),
      @"activeFps" : @(activeFps),
      @"videoRange" : @"full",
      @"colorSpace" : @"srgb",
      @"wideColorAuto" : @NO,
      @"mirrored" : @NO,
    };
    OxidePerfEmitJSONLine(kOxidePerfCameraContractSummaryPrefix, payload);
    return;
  }

  oxide_host_stats_t stats = {0};
  if (oxide_host_app_stats(&stats) != 0) {
    return;
  }
  NSDictionary *payload = @{
    @"source" : @"oxide-live-app-host",
    @"transport" :
        @"OxideAppHost+AVCaptureVideoDataOutput+CVMetalTexture(NV12)",
    @"devicePosition" : @"back",
    @"sessionPreset" : OxidePerfCameraCaptureContractPresetName(),
    @"requestedPixelFormat" : @"420f",
    @"activePixelFormat" : (stats.cam_video_range != 0 ? @"420v" : @"420f"),
    @"requestedWidth" : @(1280),
    @"requestedHeight" : @(720),
    @"requestedFps" : @(30),
    @"activeWidth" : @((int)stats.cam_width),
    @"activeHeight" : @((int)stats.cam_height),
    @"activeFps" : @(stats.cam_fps),
    @"videoRange" : (stats.cam_video_range != 0 ? @"video" : @"full"),
    @"colorSpace" : @"srgb",
    @"wideColorAuto" : @NO,
    @"mirrored" : @NO,
  };
  OxidePerfEmitJSONLine(kOxidePerfCameraContractSummaryPrefix, payload);
}

- (void)emitActualAppHostDebugSummary {
  if (OxidePerfActualAppAVFoundationCameraBenchmarkEnabled()) {
    return;
  }
  oxide_host_stats_t stats = {0};
  if (oxide_host_app_stats(&stats) != 0) {
    return;
  }
  oxide_host_app_debug_perf_t debugPerf = {0};
  if (oxide_host_app_debug_perf(&debugPerf) != 0) {
    return;
  }
  NSDictionary *payload = @{
    @"displayLinkCallbacks" : @(debugPerf.on_tick_calls),
    @"sceneWillConnectCalls" : @(debugPerf.scene_will_connect_calls),
    @"perfSceneBranchCalls" : @(debugPerf.perf_scene_branch_calls),
    @"normalSceneBranchCalls" : @(debugPerf.normal_scene_branch_calls),
    @"metalViewInstalls" : @(debugPerf.metal_view_installs),
    @"displayLinkCreateCalls" : @(debugPerf.display_link_create_calls),
    @"sceneDidBecomeActiveCalls" : @(debugPerf.scene_did_become_active_calls),
    @"sceneWillEnterForegroundCalls" :
        @(debugPerf.scene_will_enter_foreground_calls),
    @"ensureHostInitializedCalls" : @(debugPerf.ensure_host_initialized_calls),
    @"hostReadyTransitions" : @(debugPerf.host_ready_transitions),
    @"onTickCalls" : @(debugPerf.on_tick_calls),
    @"cameraGenerationAdvances" : @(stats.cam_generation_advances),
    @"cameraFrameTriggeredRenders" :
        @(debugPerf.camera_frame_triggered_renders),
    @"planSkips" : @(debugPerf.plan_skips),
    @"drawablesAcquired" : @(debugPerf.drawables_acquired),
    @"commandBuffersCommitted" : @(debugPerf.command_buffers_committed),
    @"previewSubmissionDepth" : @(stats.renderer_preview_submission_depth),
    @"presentedFrameAgeMs" : @(stats.renderer_preview_submission_frame_age_ms),
    @"samplesReceived" : @(stats.cam_samples_received),
    @"samplesDroppedPrebridge" : @(stats.cam_samples_dropped_prebridge),
    @"samplesBridged" : @(stats.cam_samples_bridged),
    @"samplesPublished" : @(stats.cam_samples_published),
    @"samplesPresented" : @(stats.cam_samples_presented),
    @"samplesSupersededBeforePresent" :
        @(stats.cam_samples_superseded_before_present),
    @"runningUiTest" : @(debugPerf.running_ui_test != 0),
    @"runningPerfBenchmarkHost" : @(debugPerf.running_perf_benchmark_host != 0),
    @"shouldRender" : @(debugPerf.should_render != 0),
    @"hostReady" : @(gHostAppReady),
  };
  OxidePerfEmitJSONLine(kOxidePerfAppHostDebugSummaryPrefix, payload);
}

- (BOOL)configureActualAppAVFoundationSessionIfNeeded {
  if (self.perfBenchmarkAVFoundationSession != nil) {
    return YES;
  }
  OxidePerfCameraPreviewView *previewView = self.perfCameraPreviewView;
  if (previewView == nil) {
    return NO;
  }
  AVCaptureDeviceDiscoverySession *discovery = [AVCaptureDeviceDiscoverySession
      discoverySessionWithDeviceTypes:@[
        AVCaptureDeviceTypeBuiltInWideAngleCamera
      ]
                            mediaType:AVMediaTypeVideo
                             position:AVCaptureDevicePositionBack];
  AVCaptureDevice *device = discovery.devices.firstObject;
  if (device == nil) {
    device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
  }
  if (device == nil) {
    OXLOG(@"actual-app AVFoundation benchmark: missing video device");
    return NO;
  }
  NSError *error = nil;
  AVCaptureDeviceInput *input =
      [AVCaptureDeviceInput deviceInputWithDevice:device error:&error];
  if (error != nil || input == nil) {
    OXLOG(@"actual-app AVFoundation benchmark: failed to create input %@",
          error);
    return NO;
  }
  AVCaptureSession *session = [AVCaptureSession new];
  [session beginConfiguration];
  if ([OxidePerfCameraCaptureContractPresetName()
          isEqualToString:@"hd1280x720"]) {
    if ([session canSetSessionPreset:AVCaptureSessionPreset1280x720]) {
      session.sessionPreset = AVCaptureSessionPreset1280x720;
    }
  } else if ([session
                 canSetSessionPreset:AVCaptureSessionPresetInputPriority]) {
    session.sessionPreset = AVCaptureSessionPresetInputPriority;
  }
  if (![session canAddInput:input]) {
    [session commitConfiguration];
    OXLOG(@"actual-app AVFoundation benchmark: cannot add device input");
    return NO;
  }
  [session addInput:input];
  if ([device lockForConfiguration:&error]) {
    CMTime frameDuration = CMTimeMake(1, 30);
    AVCaptureDeviceFormat *bestFormat = nil;
    int bestHeightDiff = INT_MAX;
    int bestWidthDiff = INT_MAX;
    int bestRangeRank = INT_MAX;
    for (AVCaptureDeviceFormat *format in device.formats) {
      FourCharCode subtype =
          CMFormatDescriptionGetMediaSubType(format.formatDescription);
      int rangeRank = INT_MAX;
      if (subtype == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange) {
        rangeRank = 0;
      } else if (subtype == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange) {
        rangeRank = 1;
      } else {
        continue;
      }
      BOOL supportsFps = NO;
      for (AVFrameRateRange *range in format.videoSupportedFrameRateRanges) {
        if (CMTimeCompare(frameDuration, range.minFrameDuration) >= 0 &&
            CMTimeCompare(frameDuration, range.maxFrameDuration) <= 0) {
          supportsFps = YES;
          break;
        }
      }
      if (!supportsFps) {
        continue;
      }
      CMVideoDimensions dims =
          CMVideoFormatDescriptionGetDimensions(format.formatDescription);
      int heightDiff = abs((int)dims.height - 720);
      int widthDiff = abs((int)dims.width - 1280);
      if (bestFormat == nil || heightDiff < bestHeightDiff ||
          (heightDiff == bestHeightDiff && rangeRank < bestRangeRank) ||
          (heightDiff == bestHeightDiff && rangeRank == bestRangeRank &&
           widthDiff < bestWidthDiff)) {
        bestFormat = format;
        bestHeightDiff = heightDiff;
        bestWidthDiff = widthDiff;
        bestRangeRank = rangeRank;
      }
    }
    if (bestFormat != nil) {
      device.activeFormat = bestFormat;
    }
    device.activeVideoMinFrameDuration = frameDuration;
    device.activeVideoMaxFrameDuration = frameDuration;
    [device unlockForConfiguration];
  } else if (error != nil) {
    OXLOG(@"actual-app AVFoundation benchmark: device lock failed %@", error);
  }
  [session commitConfiguration];
  previewView.previewLayer.videoGravity = AVLayerVideoGravityResizeAspectFill;
  previewView.previewLayer.session = session;
  AVCaptureConnection *connection = previewView.previewLayer.connection;
  if (connection != nil) {
    connection.automaticallyAdjustsVideoMirroring = NO;
    connection.videoMirrored = NO;
    CGFloat portraitAngle = 90.0;
    if (@available(iOS 17.0, *)) {
      if ([connection isVideoRotationAngleSupported:portraitAngle]) {
        connection.videoRotationAngle = portraitAngle;
      }
    }
  }
  self.perfBenchmarkAVFoundationSession = session;
  self.perfBenchmarkAVFoundationQueue = dispatch_queue_create(
      "com.oxide.perf.real_app.avfoundation", DISPATCH_QUEUE_SERIAL);
  dispatch_sync(self.perfBenchmarkAVFoundationQueue, ^{
    [session startRunning];
  });
  return YES;
}

- (void)pollActualAppCameraBenchmarkReadiness {
  if (!OxidePerfActualAppCameraBenchmarkEnabled() || self.perfBenchmarkReady ||
      self.perfBenchmarkRunning) {
    return;
  }
  if (OxidePerfActualAppAVFoundationCameraBenchmarkEnabled()) {
    if (self.perfBenchmarkAVFoundationSession.isRunning) {
      self.perfBenchmarkReady = YES;
      [self updateActualAppCameraBenchmarkState:
                [NSString stringWithFormat:@"ready:%u",
                                           self.perfBenchmarkCompletedRuns]];
      if (!self.perfBenchmarkReadyEmitted) {
        self.perfBenchmarkReadyEmitted = YES;
        OxidePerfEmitConsoleLine(
            [NSString stringWithFormat:@"OXIDE_READY %@", OxidePerfCaseName()]);
        OxidePerfPostDarwinNotification(kOxidePerfReadyNotification);
      }
      return;
    }
  } else {
    if (!gHostAppReady) {
      if (gMetalView != nil) {
        EnsureHostInitialized(gMetalView);
      }
    }
    oxide_host_stats_t stats = {0};
    if (gHostAppReady && oxide_host_app_stats(&stats) == 0 &&
        stats.cam_running != 0 && stats.cam_paused == 0 &&
        stats.cam_width > 0 && stats.cam_height > 0 &&
        stats.cam_generation_advances > 0 && gAppDebugPerf.on_tick_calls > 0) {
      self.perfBenchmarkReady = YES;
      [self updateActualAppCameraBenchmarkState:
                [NSString stringWithFormat:@"ready:%u",
                                           self.perfBenchmarkCompletedRuns]];
      if (!self.perfBenchmarkReadyEmitted) {
        self.perfBenchmarkReadyEmitted = YES;
        OxidePerfEmitConsoleLine(
            [NSString stringWithFormat:@"OXIDE_READY %@", OxidePerfCaseName()]);
        OxidePerfPostDarwinNotification(kOxidePerfReadyNotification);
      }
      return;
    }
  }
  dispatch_after(
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.05 * NSEC_PER_SEC)),
      dispatch_get_main_queue(), ^{
        [self pollActualAppCameraBenchmarkReadiness];
      });
}

- (void)configureActualAppCameraBenchmarkIfNeeded {
  if (!OxidePerfActualAppCameraBenchmarkEnabled() ||
      self.perfBenchmarkConfigured) {
    return;
  }
  self.perfBenchmarkConfigured = YES;
  self.perfBenchmarkReady = NO;
  self.perfBenchmarkRunning = NO;
  self.perfBenchmarkCompletedRuns = 0;
  CFNotificationCenterAddObserver(
      CFNotificationCenterGetDarwinNotifyCenter(),
      (__bridge const void *)(self), OxidePerfCameraBenchmarkStartCallback,
      kOxidePerfStartNotificationCF, NULL,
      CFNotificationSuspensionBehaviorDeliverImmediately);
  [self updateActualAppCameraBenchmarkIterationStartObserver];
  [self updateActualAppCameraBenchmarkState:@"booting"];
  if (OxidePerfActualAppAVFoundationCameraBenchmarkEnabled()) {
    if (![self configureActualAppAVFoundationSessionIfNeeded]) {
      [self updateActualAppCameraBenchmarkState:@"failed"];
      return;
    }
    [self pollActualAppCameraBenchmarkReadiness];
    return;
  }

  int32_t sceneIndex = OxideResolveSceneIndexNamed("Camera");
  if (sceneIndex < 0) {
    OXLOG(@"actual-app custom benchmark: missing Camera scene");
    [self updateActualAppCameraBenchmarkState:@"failed"];
    return;
  }
  oxide_host_set_benchmark_mode(1);
  oxide_host_set_camera_render_mode(1);
  oxide_host_set_camera_texture_source(0);
  oxide_host_set_scene((uint32_t)sceneIndex);
  oxide_host_set_camera_options(0, 0.0f, 0, 0);
  oxide_host_set_camera_running_mode(1, 1);
  [self refreshStats];
  [self pollActualAppCameraBenchmarkReadiness];
}

- (void)finishActualAppCameraBenchmarkRun {
  if (!self.perfBenchmarkRunning) {
    return;
  }
  self.perfBenchmarkRunning = NO;
  self.perfBenchmarkCompletedRuns += 1;
  [self updateActualAppCameraBenchmarkIterationStartObserver];
  [self emitActualAppCameraContractSummary];
  [self emitActualAppHostDebugSummary];
  [self updateActualAppCameraBenchmarkState:
            [NSString
                stringWithFormat:@"ready:%u", self.perfBenchmarkCompletedRuns]];
  OxidePerfEmitConsoleLine(
      [NSString stringWithFormat:@"OXIDE_COMPLETE %@", OxidePerfCaseName()]);
  OxidePerfPostDarwinNotification(kOxidePerfCompleteNotification);
  OxidePerfPostDarwinNotification(kOxidePerfReadyNotification);
}

- (void)handleActualAppCameraBenchmarkStart {
  if (!OxidePerfActualAppCameraBenchmarkEnabled() || !self.perfBenchmarkReady ||
      self.perfBenchmarkRunning) {
    return;
  }
  self.perfBenchmarkRunning = YES;
  [self updateActualAppCameraBenchmarkState:
            [NSString stringWithFormat:@"running:%u",
                                       self.perfBenchmarkCompletedRuns + 1]];
  if (!OxidePerfActualAppAVFoundationCameraBenchmarkEnabled()) {
    oxide_host_reset_camera_perf_counters();
  }
  dispatch_after(
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.0 * NSEC_PER_SEC)),
      dispatch_get_main_queue(), ^{
        [self finishActualAppCameraBenchmarkRun];
      });
}

- (void)applySceneSelection:(UISegmentedControl *)control {
  if (!control || !self.hasRealScenes) {
    return;
  }
  uint32_t idx = (uint32_t)control.selectedSegmentIndex;
  if (oxide_host_set_scene(idx) != 0) {
    uint32_t current = oxide_host_current_scene();
    if (current < control.numberOfSegments) {
      [control setSelectedSegmentIndex:(NSInteger)current];
    }
  }
}

- (IBAction)sceneChanged:(UISegmentedControl *)control {
  [self applySceneSelection:control];
  NSInteger idx = control.selectedSegmentIndex;
  NSString *title =
      (idx >= 0) ? [control titleForSegmentAtIndex:(NSUInteger)idx] : nil;
  if ([title isEqualToString:@"Camera"]) {
    oxide_host_set_camera_running(1);
    if (self.camCaptureSwitch) {
      [self.camCaptureSwitch setOn:YES animated:YES];
    }
    [self pushCamOptions];
    [self refreshStats];
  } else if ([title isEqualToString:@"Animations"]) {
    [self pushAnimOptions];
  } else if ([title isEqualToString:@"Damage Lab"]) {
    [self pushDamageOptions];
  } else if ([title isEqualToString:@"Input & Haptics"]) {
    [self syncImeState];
    [self pushImeStatus:@"Input scene ready"];
  } else if ([title isEqualToString:@"Nine Slice"]) {
    [self pushNineSliceOptions];
  } else if ([title isEqualToString:@"SDF Text"]) {
    [self pushSdfOptions];
  } else if ([title isEqualToString:@"Snapshot"]) {
    [self refreshSnapshotStatusLabel];
  }

  if (![title isEqualToString:@"Camera"]) {
    [self refreshStats];
  }
}

- (IBAction)onOverlaySwitch:(UISwitch *)sw {
  uint8_t desired = sw.isOn ? 1 : 0;
  if (oxide_host_set_overlay_visible(desired) != 0) {
    [sw setOn:!sw.isOn animated:NO];
  }
}

- (IBAction)onReduceMotionSwitch:(UISwitch *)sw {
  uint8_t desired = sw.isOn ? 1 : 0;
  if (oxide_host_set_reduce_motion(desired) != 0) {
    [sw setOn:!sw.isOn animated:NO];
  }
  [self updateDisplayLinkRange];
}

- (void)pushImeStatus:(NSString *)message {
  if (!message) {
    return;
  }
  const char *utf8 = message.UTF8String;
  if (utf8) {
    oxide_host_input_log(utf8, strlen(utf8));
  }
  if (self.statusLabel) {
    self.statusLabel.text = message;
  }
}

- (void)syncImeState {
  UITextView *tv = self.imeTextView;
  if (!tv) {
    return;
  }
  NSRange sel = tv.selectedRange;
  if (sel.location != NSNotFound) {
    uint32_t start = (uint32_t)sel.location;
    uint32_t end = (uint32_t)(sel.location + sel.length);
    oxide_host_emit_text_selection(start, end);
  }
  UITextRange *marked = tv.markedTextRange;
  if (marked) {
    NSInteger startIdx = [tv offsetFromPosition:tv.beginningOfDocument
                                     toPosition:marked.start];
    NSInteger endIdx = [tv offsetFromPosition:tv.beginningOfDocument
                                   toPosition:marked.end];
    NSString *markedText = [tv textInRange:marked] ?: @"";
    NSData *data = [markedText dataUsingEncoding:NSUTF8StringEncoding];
    const char *bytes = data.length > 0 ? data.bytes : NULL;
    oxide_host_emit_text_composition((uint32_t)startIdx, (uint32_t)endIdx,
                                     bytes, (size_t)data.length);
  } else {
    uint32_t caret = (sel.location != NSNotFound) ? (uint32_t)sel.location : 0;
    oxide_host_emit_text_composition(caret, caret, NULL, 0);
  }
}

- (void)refreshStats {
  oxide_host_stats_t stats = {0};
  if (oxide_host_app_stats(&stats) != 0) {
    return;
  }
  if (self.fpsLabel) {
    float dmg = stats.damage_pct * 100.0f;
    self.fpsLabel.text = [NSString
        stringWithFormat:@"%.0f fps • draws %u • anims %u • damage %.0f%%",
                         stats.fps, stats.draws, stats.anims, dmg];
  }
  if (self.camMetricsLabel) {
    NSString *matrix = CameraMatrixName(stats.cam_matrix);
    NSString *range = CameraRangeName(stats.cam_video_range);
    float coverage = stats.cam_coverage_pct * 100.0f;
    float fps = stats.cam_fps;
    BOOL pausedFlag = (stats.cam_paused != 0) || (stats.cam_running == 0);
    NSString *paused = BoolYesNo(pausedFlag);
    NSString *lowPower = BoolYesNo(stats.cam_low_power != 0);
    self.camMetricsLabel.text =
        [NSString stringWithFormat:@"Cam %ux%u bd=%u mx=%@ rng=%@ cov=%.0f%% "
                                   @"fps=%.1f paused=%@ lp=%@ th=%u",
                                   stats.cam_width, stats.cam_height,
                                   stats.cam_bit_depth, matrix, range, coverage,
                                   fps, paused, lowPower, stats.cam_thermal];
  }
  if (self.camCaptureSwitch) {
    BOOL desired = (stats.cam_running != 0) && (stats.cam_paused == 0);
    if (self.camCaptureSwitch.isOn != desired) {
      [self.camCaptureSwitch setOn:desired animated:YES];
    }
  }
}

- (BOOL)textView:(UITextView *)textView
    shouldChangeTextInRange:(NSRange)range
            replacementText:(NSString *)text {
  if (textView != self.imeTextView) {
    return YES;
  }
  if (range.length > 0) {
    for (NSUInteger i = 0; i < range.length; ++i) {
      const char backspace = '\b';
      oxide_host_emit_text_commit(&backspace, 1);
    }
  }
  if (text.length > 0) {
    NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
    if (data.length > 0) {
      oxide_host_emit_text_commit(data.bytes, data.length);
    }
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    [self syncImeState];
  });
  return YES;
}

- (void)textViewDidChangeSelection:(UITextView *)textView {
  if (textView != self.imeTextView) {
    return;
  }
  [self syncImeState];
}

- (void)textViewDidBeginEditing:(UITextView *)textView {
  if (textView != self.imeTextView) {
    return;
  }
  UIView *root = self.window.rootViewController.view;
  UITextRange *sel = textView.selectedTextRange;
  CGRect caret = sel ? [textView caretRectForPosition:sel.start] : CGRectNull;
  if (CGRectIsNull(caret) || CGRectIsEmpty(caret)) {
    caret = textView.bounds;
  }
  CGRect converted = [textView convertRect:caret toView:root];
  oxide_host_emit_ime_shown(converted.origin.x, converted.origin.y,
                            converted.size.width, converted.size.height);
  [self pushImeStatus:@"IME shown"];
  [self syncImeState];
}

- (void)textViewDidEndEditing:(UITextView *)textView {
  if (textView != self.imeTextView) {
    return;
  }
  oxide_host_emit_ime_hidden();
  [self pushImeStatus:@"IME hidden"];
  [self syncImeState];
}

- (void)pushCamOptions {
  uint8_t blur = self.camBlurSwitch.isOn ? 1 : 0;
  uint8_t gray = self.camGraySwitch.isOn ? 1 : 0;
  uint8_t anim = self.camAnimSwitch.isOn ? 1 : 0;
  float sigma = self.camSigmaSlider.value;
  oxide_host_set_camera_options(blur, sigma, gray, anim);
  [self refreshStats];
}

- (IBAction)onCamBlur:(UISwitch *)sw {
  (void)sw;
  [self pushCamOptions];
}
- (IBAction)onCamGray:(UISwitch *)sw {
  (void)sw;
  [self pushCamOptions];
}
- (IBAction)onCamAnim:(UISwitch *)sw {
  (void)sw;
  [self pushCamOptions];
}
- (IBAction)onCamSigma:(UISlider *)slider {
  (void)slider;
  [self pushCamOptions];
}
- (IBAction)onCamCapture:(UISwitch *)sw {
  uint8_t desired = sw.isOn ? 1 : 0;
  if (oxide_host_set_camera_running(desired) != 0) {
    [sw setOn:!sw.isOn animated:NO];
  }
  [self refreshStats];
}

- (void)pushNineSliceOptions {
  if (!self.nineSliceSlider || !self.nineAlphaSlider) {
    return;
  }
  oxide_host_set_nine_slice(self.nineSliceSlider.value,
                            self.nineAlphaSlider.value);
}

- (IBAction)onNineSlice:(UISlider *)slider {
  (void)slider;
  [self pushNineSliceOptions];
}
- (IBAction)onNineAlpha:(UISlider *)slider {
  (void)slider;
  [self pushNineSliceOptions];
}

- (void)pushSdfOptions {
  if (!self.sdfSlider) {
    return;
  }
  oxide_host_set_sdf_font(self.sdfSlider.value);
}

- (IBAction)onSdfFont:(UISlider *)slider {
  (void)slider;
  [self pushSdfOptions];
}

- (IBAction)onImeFocus:(UIButton *)button {
  (void)button;
  if (self.imeTextView) {
    [self.imeTextView becomeFirstResponder];
    [self pushImeStatus:@"IME focus requested"];
    [self syncImeState];
  }
}

- (IBAction)onImeBlur:(UIButton *)button {
  (void)button;
  if (self.imeTextView) {
    [self.imeTextView resignFirstResponder];
    [self pushImeStatus:@"IME blur requested"];
    [self syncImeState];
  }
}

- (IBAction)onImeCopy:(UIButton *)button {
  (void)button;
  NSString *text = self.imeTextView.text ?: @"";
  NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
  if (data.length > 0) {
    oxide_host_clipboard_set(data.bytes, data.length);
  } else {
    oxide_host_clipboard_set("", 0);
  }
  NSString *msg = [NSString
      stringWithFormat:@"Copied %lu chars", (unsigned long)text.length];
  [self pushImeStatus:msg];
}

- (IBAction)onImePaste:(UIButton *)button {
  (void)button;
  char *buf = NULL;
  size_t len = 0;
  if (oxide_host_clipboard_get(&buf, &len)) {
    NSString *value = [[NSString alloc] initWithBytes:buf
                                               length:len
                                             encoding:NSUTF8StringEncoding];
    oxide_host_string_free(buf);
    if (value) {
      self.imeTextView.text = value;
      [self syncImeState];
      NSString *msg = [NSString
          stringWithFormat:@"Pasted %lu chars", (unsigned long)value.length];
      [self pushImeStatus:msg];
      return;
    }
  }
  [self pushImeStatus:@"Paste failed"];
}

- (IBAction)onImeHaptic:(UIButton *)button {
  (void)button;
  oxide_host_haptics_play(1);
  [self pushImeStatus:@"Haptic played"];
}

- (void)refreshSnapshotStatusLabelWithFallback:(NSString *)fallback {
  if (!self.statusLabel) {
    return;
  }
  char message[512] = {0};
  uint32_t len =
      oxide_host_get_snapshot_status(message, (uint32_t)sizeof(message));
  NSString *status = [NSString stringWithUTF8String:message];
  if (!status || len == 0) {
    status = fallback ? fallback : @"";
  }
  self.statusLabel.text = status;
}

- (void)refreshSnapshotStatusLabel {
  [self refreshSnapshotStatusLabelWithFallback:nil];
}

- (IBAction)onSnapshotButton:(UIButton *)button {
  (void)button;
  int32_t rc = oxide_host_take_snapshot();
  NSString *fallback = (rc == 0) ? @"Snapshot saved" : @"Snapshot failed";
  [self refreshSnapshotStatusLabelWithFallback:fallback];
}

- (void)pushAnimOptions {
  if (!self.animPlaySwitch || !self.animPhaseSlider) {
    return;
  }
  oxide_host_set_anim_play(self.animPlaySwitch.isOn ? 1 : 0);
  oxide_host_set_anim_progress(self.animPhaseSlider.value);
}

- (IBAction)onAnimPlay:(UISwitch *)sw {
  (void)sw;
  [self pushAnimOptions];
}
- (IBAction)onAnimPhase:(UISlider *)slider {
  (void)slider;
  [self pushAnimOptions];
}

- (void)pushDamageOptions {
  if (!self.damageEnableSwitch || !self.damageUseSlider ||
      !self.damagePrefSlider) {
    return;
  }
  oxide_host_set_damage_options(self.damageEnableSwitch.isOn ? 1 : 0,
                                self.damageUseSlider.value,
                                self.damagePrefSlider.value);
}

- (IBAction)onDamageEnable:(UISwitch *)sw {
  (void)sw;
  [self pushDamageOptions];
}
- (IBAction)onDamageUse:(UISlider *)slider {
  (void)slider;
  [self pushDamageOptions];
}
- (IBAction)onDamagePref:(UISlider *)slider {
  (void)slider;
  [self pushDamageOptions];
}

- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connectionOptions {
  (void)session;
  (void)connectionOptions;
  if (![scene isKindOfClass:[UIWindowScene class]]) {
    return;
  }
  UIWindowScene *ws = (UIWindowScene *)scene;
  gAppDebugPerf.scene_will_connect_calls += 1;
  gAppDebugPerf.running_ui_test = IsRunningUITest() ? 1 : 0;
  gAppDebugPerf.running_perf_benchmark_host =
      IsRunningPerfBenchmarkHost() ? 1 : 0;
  gAppDebugPerf.should_render = ShouldRender() ? 1 : 0;
  self.window = [[OxideTouchWindow alloc] initWithWindowScene:ws];
  UIViewController *vc = [UIViewController new];
  if (IsRunningPerfBenchmarkHost()) {
    gAppDebugPerf.perf_scene_branch_calls += 1;
    if (!OxidePerfCameraRealAppHostEnabled()) {
      UIView *blank = [UIView new];
      blank.backgroundColor = [UIColor whiteColor];
      vc.view = blank;
      self.window.rootViewController = vc;
      [self.window makeKeyAndVisible];
      return;
    }
    UIView *container = [UIView new];
    container.backgroundColor = [UIColor whiteColor];
    MetalView *mv = nil;
    BOOL useAVFoundationVisiblePreview =
        OxidePerfActualAppAVFoundationCameraBenchmarkEnabled();
    if (!useAVFoundationVisiblePreview) {
      mv = [MetalView new];
      mv.translatesAutoresizingMaskIntoConstraints = NO;
      mv.backgroundColor = [UIColor whiteColor];
      [container addSubview:mv];
      [NSLayoutConstraint activateConstraints:@[
        [mv.leadingAnchor constraintEqualToAnchor:container.leadingAnchor],
        [mv.trailingAnchor constraintEqualToAnchor:container.trailingAnchor],
        [mv.topAnchor constraintEqualToAnchor:container.topAnchor],
        [mv.bottomAnchor constraintEqualToAnchor:container.bottomAnchor],
      ]];
    }
    if (OxidePerfCameraRealAppHybridVisiblePreviewEnabled() ||
        useAVFoundationVisiblePreview) {
      OxidePerfCameraPreviewView *previewView =
          [OxidePerfCameraPreviewView new];
      previewView.translatesAutoresizingMaskIntoConstraints = NO;
      previewView.userInteractionEnabled = NO;
      previewView.backgroundColor = useAVFoundationVisiblePreview
                                        ? [UIColor blackColor]
                                        : [UIColor clearColor];
      previewView.previewLayer.videoGravity =
          AVLayerVideoGravityResizeAspectFill;
      [container addSubview:previewView];
      [NSLayoutConstraint activateConstraints:@[
        [previewView.leadingAnchor
            constraintEqualToAnchor:container.leadingAnchor],
        [previewView.trailingAnchor
            constraintEqualToAnchor:container.trailingAnchor],
        [previewView.topAnchor constraintEqualToAnchor:container.topAnchor],
        [previewView.bottomAnchor
            constraintEqualToAnchor:container.bottomAnchor],
      ]];
      self.perfCameraPreviewView = previewView;
    } else {
      self.perfCameraPreviewView = nil;
    }
    if (OxidePerfActualAppCameraBenchmarkEnabled() || IsRunningUITest()) {
      [self installPerfBenchmarkStateLabelIfNeededInView:container];
    } else {
      self.perfBenchmarkStateLabel = nil;
    }
    vc.view = container;
    self.window.rootViewController = vc;
    gMetalView = mv;
    if (mv != nil) {
      gAppDebugPerf.metal_view_installs += 1;
    }
    [self.window makeKeyAndVisible];
    [self installCameraDrivenSchedulingCallbackIfNeeded];
    if (mv != nil && ShouldRender()) {
      OXLOG(@"willConnect: creating DisplayLink for perf app host");
      self.displayLink =
          [CADisplayLink displayLinkWithTarget:self
                                      selector:@selector(onTick:)];
      gAppDebugPerf.display_link_create_calls += 1;
      [self updateDisplayLinkRange];
      [self.displayLink addToRunLoop:[NSRunLoop mainRunLoop]
                             forMode:NSRunLoopCommonModes];
      EnsureHostInitialized(mv);
    }
    [self configureActualAppCameraBenchmarkIfNeeded];
    return;
  }
  gAppDebugPerf.normal_scene_branch_calls += 1;
  MetalView *mv = [MetalView new];
  mv.backgroundColor = [UIColor whiteColor];
  vc.view = mv;
  gMetalView = mv;
  gAppDebugPerf.metal_view_installs += 1;
  self.window.rootViewController = vc;
  if (OxidePerfActualAppCameraBenchmarkEnabled() || IsRunningUITest()) {
    [self installPerfBenchmarkStateLabelIfNeededInView:vc.view];
  } else {
    self.perfBenchmarkStateLabel = nil;
  }
  UILabel *fps = [UILabel new];
  fps.translatesAutoresizingMaskIntoConstraints = NO;
  fps.font = [UIFont monospacedDigitSystemFontOfSize:12
                                              weight:UIFontWeightSemibold];
  fps.textColor = [UIColor colorWithWhite:0.1 alpha:0.95];
  fps.backgroundColor = [UIColor colorWithWhite:1.0 alpha:0.75];
  fps.layer.cornerRadius = 4.0;
  fps.layer.masksToBounds = YES;
  fps.text = @"-- fps";
  [vc.view addSubview:fps];
  [NSLayoutConstraint activateConstraints:@[
    [fps.topAnchor constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.topAnchor
                                  constant:8.0],
    [fps.leadingAnchor
        constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.leadingAnchor
                       constant:8.0]
  ]];
  self.fpsLabel = fps;

  // Always show a small on-screen log label during debugging
  UILabel *ll = [UILabel new];
  ll.translatesAutoresizingMaskIntoConstraints = NO;
  ll.font = [UIFont monospacedSystemFontOfSize:9 weight:UIFontWeightRegular];
  ll.textColor = [UIColor colorWithWhite:0.95 alpha:1.0];
  ll.backgroundColor = [[UIColor blackColor] colorWithAlphaComponent:0.6];
  ll.numberOfLines = 0;
  ll.layer.cornerRadius = 4.0;
  ll.layer.masksToBounds = YES;
  ll.text = @"UILog ready";
  [vc.view addSubview:ll];
  [NSLayoutConstraint activateConstraints:@[
    [ll.leadingAnchor
        constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.leadingAnchor
                       constant:8.0],
    [ll.bottomAnchor
        constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.bottomAnchor
                       constant:-8.0],
    [ll.widthAnchor constraintLessThanOrEqualToConstant:360.0]
  ]];
  gUILogLabel = ll;

  NSMutableArray<NSString *> *sceneItems = [NSMutableArray array];
  uint32_t sceneCount = oxide_host_scene_count();
  for (uint32_t i = 0; i < sceneCount; ++i) {
    uint32_t need = oxide_host_scene_name(i, NULL, 0);
    if (need == 0) {
      continue;
    }
    NSMutableData *buf = [NSMutableData dataWithLength:need];
    if (oxide_host_scene_name(i, buf.mutableBytes, need) != 0) {
      NSString *name = [NSString stringWithUTF8String:buf.mutableBytes];
      if (name) {
        [sceneItems addObject:name];
      }
    }
  }
  self.hasRealScenes = sceneItems.count > 0;
  if (!self.hasRealScenes) {
    NSLog(@"[Oxide] no scenes reported (count=%u)", sceneCount);
    [sceneItems addObject:@"Default Scene"];
  }
  UISegmentedControl *seg =
      [[UISegmentedControl alloc] initWithItems:sceneItems];
  seg.translatesAutoresizingMaskIntoConstraints = NO;
  seg.selectedSegmentIndex = (NSInteger)oxide_host_current_scene();
  if (!self.hasRealScenes) {
    seg.enabled = NO;
  }
  if (@available(iOS 14.0, *)) {
    __weak typeof(self) weakSelf = self;
    __weak UISegmentedControl *weakSeg = seg;
    UIAction *action =
        [UIAction actionWithHandler:^(__kindof UIAction *_Nonnull sceneAction) {
          (void)sceneAction;
          typeof(self) strongSelf = weakSelf;
          UISegmentedControl *strongSeg = weakSeg;
          if (!strongSelf || !strongSeg) {
            return;
          }
          [strongSelf sceneChanged:strongSeg];
        }];
    [seg addAction:action forControlEvents:UIControlEventValueChanged];
  } else {
    [seg addTarget:self
                  action:@selector(sceneChanged:)
        forControlEvents:UIControlEventValueChanged];
  }
  seg.accessibilityIdentifier = @"sceneControl";
  self.sceneControl = seg;

  UILabel *overlayLabel = [UILabel new];
  overlayLabel.text = @"Overlay";
  overlayLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISwitch *overlaySwitch = [UISwitch new];
  overlaySwitch.on = YES;
  [overlaySwitch addTarget:self
                    action:@selector(onOverlaySwitch:)
          forControlEvents:UIControlEventValueChanged];
  overlaySwitch.accessibilityIdentifier = @"overlaySwitch";
  self.overlaySwitch = overlaySwitch;

  UILabel *reduceLabel = [UILabel new];
  reduceLabel.text = @"Reduce Motion";
  reduceLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISwitch *reduceSwitch = [UISwitch new];
  reduceSwitch.on = NO;
  [reduceSwitch addTarget:self
                   action:@selector(onReduceMotionSwitch:)
         forControlEvents:UIControlEventValueChanged];
  reduceSwitch.accessibilityIdentifier = @"reduceMotionSwitch";
  self.reduceSwitch = reduceSwitch;

  UIStackView *overlayRow = [[UIStackView alloc]
      initWithArrangedSubviews:@[ overlayLabel, overlaySwitch ]];
  overlayRow.axis = UILayoutConstraintAxisHorizontal;
  overlayRow.spacing = 6.0;

  UIStackView *reduceRow = [[UIStackView alloc]
      initWithArrangedSubviews:@[ reduceLabel, reduceSwitch ]];
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
  [imeFocus addTarget:self
                action:@selector(onImeFocus:)
      forControlEvents:UIControlEventTouchUpInside];

  UIButton *imeBlur = [UIButton buttonWithType:UIButtonTypeSystem];
  [imeBlur setTitle:@"Blur" forState:UIControlStateNormal];
  imeBlur.accessibilityIdentifier = @"imeBlurButton";
  [imeBlur addTarget:self
                action:@selector(onImeBlur:)
      forControlEvents:UIControlEventTouchUpInside];

  UIButton *imeCopy = [UIButton buttonWithType:UIButtonTypeSystem];
  [imeCopy setTitle:@"Copy" forState:UIControlStateNormal];
  imeCopy.accessibilityIdentifier = @"imeCopyButton";
  [imeCopy addTarget:self
                action:@selector(onImeCopy:)
      forControlEvents:UIControlEventTouchUpInside];

  UIButton *imePaste = [UIButton buttonWithType:UIButtonTypeSystem];
  [imePaste setTitle:@"Paste" forState:UIControlStateNormal];
  imePaste.accessibilityIdentifier = @"imePasteButton";
  [imePaste addTarget:self
                action:@selector(onImePaste:)
      forControlEvents:UIControlEventTouchUpInside];

  UIButton *imeHaptic = [UIButton buttonWithType:UIButtonTypeSystem];
  [imeHaptic setTitle:@"Haptic" forState:UIControlStateNormal];
  imeHaptic.accessibilityIdentifier = @"imeHapticButton";
  [imeHaptic addTarget:self
                action:@selector(onImeHaptic:)
      forControlEvents:UIControlEventTouchUpInside];

  UIStackView *imeButtons = [[UIStackView alloc] initWithArrangedSubviews:@[
    imeFocus, imeBlur, imeCopy, imePaste, imeHaptic
  ]];
  imeButtons.axis = UILayoutConstraintAxisHorizontal;
  imeButtons.spacing = 6.0;

  UIStackView *inputGroup = [[UIStackView alloc]
      initWithArrangedSubviews:@[ inputLabel, imeView, imeButtons ]];
  inputGroup.axis = UILayoutConstraintAxisVertical;
  inputGroup.spacing = 4.0;
  inputGroup.alignment = UIStackViewAlignmentFill;

  UILabel *animPlayLabel = [UILabel new];
  animPlayLabel.text = @"Anim Play";
  animPlayLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISwitch *animPlay = [UISwitch new];
  animPlay.on = YES;
  [animPlay addTarget:self
                action:@selector(onAnimPlay:)
      forControlEvents:UIControlEventValueChanged];
  animPlay.accessibilityIdentifier = @"animationPlaySwitch";
  self.animPlaySwitch = animPlay;

  UILabel *animPhaseLabel = [UILabel new];
  animPhaseLabel.text = @"Anim Phase";
  animPhaseLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISlider *animPhase = [UISlider new];
  animPhase.minimumValue = 0.0f;
  animPhase.maximumValue = 1.0f;
  animPhase.value = 0.0f;
  [animPhase addTarget:self
                action:@selector(onAnimPhase:)
      forControlEvents:UIControlEventValueChanged];
  animPhase.accessibilityIdentifier = @"animationPhaseSlider";
  self.animPhaseSlider = animPhase;

  UIStackView *animRow1 = [[UIStackView alloc]
      initWithArrangedSubviews:@[ animPlayLabel, animPlay ]];
  animRow1.axis = UILayoutConstraintAxisHorizontal;
  animRow1.spacing = 6.0;

  UIStackView *animRow2 = [[UIStackView alloc]
      initWithArrangedSubviews:@[ animPhaseLabel, animPhase ]];
  animRow2.axis = UILayoutConstraintAxisHorizontal;
  animRow2.spacing = 6.0;

  UILabel *damageLabel = [UILabel new];
  damageLabel.text = @"Damage";
  damageLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISwitch *damageSwitch = [UISwitch new];
  damageSwitch.on = NO;
  [damageSwitch addTarget:self
                   action:@selector(onDamageEnable:)
         forControlEvents:UIControlEventValueChanged];
  damageSwitch.accessibilityIdentifier = @"damageEnableSwitch";
  self.damageEnableSwitch = damageSwitch;

  UILabel *damageUseLabel = [UILabel new];
  damageUseLabel.text = @"Use Thresh";
  damageUseLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISlider *damageUse = [UISlider new];
  damageUse.minimumValue = 0.0f;
  damageUse.maximumValue = 1.0f;
  damageUse.value = 0.70f;
  [damageUse addTarget:self
                action:@selector(onDamageUse:)
      forControlEvents:UIControlEventValueChanged];
  damageUse.accessibilityIdentifier = @"damageUseSlider";
  self.damageUseSlider = damageUse;

  UILabel *damagePrefLabel = [UILabel new];
  damagePrefLabel.text = @"Prefilter";
  damagePrefLabel.font = [UIFont systemFontOfSize:12
                                           weight:UIFontWeightRegular];
  UISlider *damagePref = [UISlider new];
  damagePref.minimumValue = 0.0f;
  damagePref.maximumValue = 1.0f;
  damagePref.value = 0.25f;
  [damagePref addTarget:self
                 action:@selector(onDamagePref:)
       forControlEvents:UIControlEventValueChanged];
  damagePref.accessibilityIdentifier = @"damagePrefSlider";
  self.damagePrefSlider = damagePref;

  UIStackView *damageRow0 = [[UIStackView alloc]
      initWithArrangedSubviews:@[ damageLabel, damageSwitch ]];
  damageRow0.axis = UILayoutConstraintAxisHorizontal;
  damageRow0.spacing = 6.0;

  UIStackView *damageRow1 = [[UIStackView alloc]
      initWithArrangedSubviews:@[ damageUseLabel, damageUse ]];
  damageRow1.axis = UILayoutConstraintAxisHorizontal;
  damageRow1.spacing = 6.0;

  UIStackView *damageRow2 = [[UIStackView alloc]
      initWithArrangedSubviews:@[ damagePrefLabel, damagePref ]];
  damageRow2.axis = UILayoutConstraintAxisHorizontal;
  damageRow2.spacing = 6.0;

  UILabel *nineLabel = [UILabel new];
  nineLabel.text = @"Nine Slice";
  nineLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISlider *nineSlice = [UISlider new];
  nineSlice.minimumValue = 0.0f;
  nineSlice.maximumValue = 40.0f;
  nineSlice.value = 16.0f;
  [nineSlice addTarget:self
                action:@selector(onNineSlice:)
      forControlEvents:UIControlEventValueChanged];
  nineSlice.accessibilityIdentifier = @"nineSliceSlider";
  self.nineSliceSlider = nineSlice;

  UILabel *nineAlphaLabel = [UILabel new];
  nineAlphaLabel.text = @"NS Alpha";
  nineAlphaLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISlider *nineAlpha = [UISlider new];
  nineAlpha.minimumValue = 0.1f;
  nineAlpha.maximumValue = 1.0f;
  nineAlpha.value = 1.0f;
  [nineAlpha addTarget:self
                action:@selector(onNineAlpha:)
      forControlEvents:UIControlEventValueChanged];
  nineAlpha.accessibilityIdentifier = @"nineAlphaSlider";
  self.nineAlphaSlider = nineAlpha;

  UIStackView *nineRow1 =
      [[UIStackView alloc] initWithArrangedSubviews:@[ nineLabel, nineSlice ]];
  nineRow1.axis = UILayoutConstraintAxisHorizontal;
  nineRow1.spacing = 6.0;

  UIStackView *nineRow2 = [[UIStackView alloc]
      initWithArrangedSubviews:@[ nineAlphaLabel, nineAlpha ]];
  nineRow2.axis = UILayoutConstraintAxisHorizontal;
  nineRow2.spacing = 6.0;

  UILabel *sdfLabel = [UILabel new];
  sdfLabel.text = @"SDF Font";
  sdfLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISlider *sdfSlider = [UISlider new];
  sdfSlider.minimumValue = 16.0f;
  sdfSlider.maximumValue = 72.0f;
  sdfSlider.value = 32.0f;
  [sdfSlider addTarget:self
                action:@selector(onSdfFont:)
      forControlEvents:UIControlEventValueChanged];
  sdfSlider.accessibilityIdentifier = @"sdfFontSlider";
  self.sdfSlider = sdfSlider;

  UIStackView *sdfRow =
      [[UIStackView alloc] initWithArrangedSubviews:@[ sdfLabel, sdfSlider ]];
  sdfRow.axis = UILayoutConstraintAxisHorizontal;
  sdfRow.spacing = 6.0;

  UIButton *snapshotButton = [UIButton buttonWithType:UIButtonTypeSystem];
  [snapshotButton setTitle:@"Capture Snapshot" forState:UIControlStateNormal];
  snapshotButton.accessibilityIdentifier = @"snapshotButton";
  [snapshotButton addTarget:self
                     action:@selector(onSnapshotButton:)
           forControlEvents:UIControlEventTouchUpInside];
  self.snapshotButton = snapshotButton;

  UIStackView *snapshotRow =
      [[UIStackView alloc] initWithArrangedSubviews:@[ snapshotButton ]];
  snapshotRow.axis = UILayoutConstraintAxisHorizontal;
  snapshotRow.spacing = 6.0;

  // Camera controls (always visible for UITests)
  UILabel *camLabel = [UILabel new];
  camLabel.text = @"Camera";
  camLabel.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISwitch *camBlur = [UISwitch new];
  camBlur.on = YES;
  [camBlur addTarget:self
                action:@selector(onCamBlur:)
      forControlEvents:UIControlEventValueChanged];
  camBlur.accessibilityIdentifier = @"cameraBlurSwitch";
  self.camBlurSwitch = camBlur;
  UISwitch *camGray = [UISwitch new];
  camGray.on = NO;
  [camGray addTarget:self
                action:@selector(onCamGray:)
      forControlEvents:UIControlEventValueChanged];
  camGray.accessibilityIdentifier = @"cameraGraySwitch";
  self.camGraySwitch = camGray;
  UISwitch *camAnim = [UISwitch new];
  camAnim.on = YES;
  [camAnim addTarget:self
                action:@selector(onCamAnim:)
      forControlEvents:UIControlEventValueChanged];
  camAnim.accessibilityIdentifier = @"cameraAnimateSwitch";
  self.camAnimSwitch = camAnim;
  UISwitch *camCapture = [UISwitch new];
  camCapture.on = YES;
  [camCapture addTarget:self
                 action:@selector(onCamCapture:)
       forControlEvents:UIControlEventValueChanged];
  camCapture.accessibilityIdentifier = @"cameraCaptureSwitch";
  self.camCaptureSwitch = camCapture;
  UILabel *sigmaLbl = [UILabel new];
  sigmaLbl.text = @"Sigma";
  sigmaLbl.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  UISlider *sigma = [UISlider new];
  sigma.minimumValue = 0.0f;
  sigma.maximumValue = 16.0f;
  sigma.value = 6.0f;
  [sigma addTarget:self
                action:@selector(onCamSigma:)
      forControlEvents:UIControlEventValueChanged];
  sigma.accessibilityIdentifier = @"cameraSigmaSlider";
  self.camSigmaSlider = sigma;

  UIStackView *camRow1 = [[UIStackView alloc] initWithArrangedSubviews:@[
    camLabel, camCapture, camBlur, camGray, camAnim
  ]];
  camRow1.axis = UILayoutConstraintAxisHorizontal;
  camRow1.spacing = 6.0;
  UIStackView *camRow2 =
      [[UIStackView alloc] initWithArrangedSubviews:@[ sigmaLbl, sigma ]];
  camRow2.axis = UILayoutConstraintAxisHorizontal;
  camRow2.spacing = 6.0;

  UILabel *camMetrics = [UILabel new];
  camMetrics.font = [UIFont systemFontOfSize:12 weight:UIFontWeightRegular];
  camMetrics.textColor = [UIColor colorWithWhite:0.15 alpha:1.0];
  camMetrics.numberOfLines = 2;
  camMetrics.text = @"Cam 0x0 bd=0 mx=709 rng=full cov=0% fps=0.0 paused=yes";
  camMetrics.accessibilityIdentifier = @"cameraMetricsLabel";
  self.camMetricsLabel = camMetrics;
  UIStackView *camMetricsRow =
      [[UIStackView alloc] initWithArrangedSubviews:@[ camMetrics ]];
  camMetricsRow.axis = UILayoutConstraintAxisHorizontal;
  camMetricsRow.spacing = 0.0;

  UIStackView *controls = [[UIStackView alloc] initWithArrangedSubviews:@[
    seg, overlayRow, reduceRow, inputGroup, animRow1, animRow2, damageRow0,
    damageRow1, damageRow2, nineRow1, nineRow2, sdfRow, snapshotRow, camRow1,
    camRow2, camMetricsRow
  ]];
  controls.axis = UILayoutConstraintAxisVertical;
  controls.spacing = 8.0;
  controls.alignment = UIStackViewAlignmentLeading;
  controls.translatesAutoresizingMaskIntoConstraints = NO;
  [vc.view addSubview:controls];
  [NSLayoutConstraint activateConstraints:@[
    [controls.leadingAnchor
        constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.leadingAnchor
                       constant:12.0],
    [controls.trailingAnchor
        constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.trailingAnchor
                       constant:-12.0],
    [controls.topAnchor
        constraintEqualToAnchor:vc.view.safeAreaLayoutGuide.topAnchor
                       constant:12.0],
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
    [status.topAnchor constraintEqualToAnchor:controls.bottomAnchor
                                     constant:12.0],
    [status.leadingAnchor constraintEqualToAnchor:controls.leadingAnchor],
    [status.trailingAnchor
        constraintLessThanOrEqualToAnchor:controls.trailingAnchor]
  ]];
  self.statusLabel = status;
  if (OxideHostChromeHidden()) {
    fps.hidden = YES;
    ll.hidden = !OxideTouchScreenLogEnabled();
    controls.hidden = YES;
    status.hidden = YES;
  }
  [self.window makeKeyAndVisible];
  [self installCameraDrivenSchedulingCallbackIfNeeded];

  [self pushAnimOptions];
  [self pushDamageOptions];
  [self pushNineSliceOptions];
  [self pushSdfOptions];
  [self pushCamOptions];
  [self refreshStats];

  [[NSNotificationCenter defaultCenter]
      addObserver:self
         selector:@selector(onPowerStateChanged:)
             name:NSProcessInfoPowerStateDidChangeNotification
           object:nil];
  if (ShouldRender() && (!IsRunningUITest() || IsRunningPerfBenchmarkHost())) {
    OXLOG(@"willConnect: creating DisplayLink");
    self.displayLink = [CADisplayLink displayLinkWithTarget:self
                                                   selector:@selector(onTick:)];
    gAppDebugPerf.display_link_create_calls += 1;
    [self updateDisplayLinkRange];
    [self.displayLink addToRunLoop:[NSRunLoop mainRunLoop]
                           forMode:NSRunLoopCommonModes];
    EnsureHostInitialized(mv);
    [self pushCamOptions];
    [self updateCameraDrivenDisplayLinkState];
  } else if (ShouldRender()) {
    OXLOG(@"willConnect: initializing host without DisplayLink under UITest");
    EnsureHostInitialized(mv);
  } else {
    OXLOG(@"willConnect: running under UITest — no DisplayLink");
  }
}

- (void)updateDisplayLinkRange {
  if (!self.displayLink) {
    return;
  }
  int fps = CurrentTargetFPS();
  if (@available(iOS 15.0, *)) {
    self.displayLink.preferredFrameRateRange =
        CAFrameRateRangeMake(fps, fps, fps);
  } else {
    self.displayLink.preferredFramesPerSecond = fps;
  }
}

- (void)onPowerStateChanged:(NSNotification *)note {
  (void)note;
  [self updateDisplayLinkRange];
}

- (void)sceneDidBecomeActive:(UIScene *)scene {
  (void)scene;
  gAppDebugPerf.scene_did_become_active_calls += 1;
  self.fpsLastSample = CACurrentMediaTime();
  self.fpsCount = 0;
  if (!IsRunningUITest() || IsRunningPerfBenchmarkHost()) {
    OXLOG(@"sceneDidBecomeActive: resuming DisplayLink");
    self.displayLink.paused = NO;
    EnsureHostInitialized(gMetalView);
    [self configureActualAppCameraBenchmarkIfNeeded];
    self.overlaySwitch.on = oxide_host_is_overlay_visible() != 0;
    self.reduceSwitch.on = oxide_host_is_reduce_motion() != 0;
    [self updateDisplayLinkRange];
    [self updateCameraDrivenDisplayLinkState];
  } else {
    OXLOG(@"sceneDidBecomeActive: under UITest");
  }
  [self refreshStats];
}

- (void)sceneWillResignActive:(UIScene *)scene {
  (void)scene;
  if (self.displayLink) {
    OXLOG(@"sceneWillResignActive: pausing DisplayLink");
    self.displayLink.paused = YES;
  }
}

- (void)sceneDidEnterBackground:(UIScene *)scene {
  (void)scene;
  if (self.displayLink) {
    OXLOG(@"sceneDidEnterBackground: pausing DisplayLink");
    self.displayLink.paused = YES;
  }
  oxide_host_app_did_enter_background();
}

- (void)sceneWillEnterForeground:(UIScene *)scene {
  (void)scene;
  gAppDebugPerf.scene_will_enter_foreground_calls += 1;
  if (!self.displayLink) {
    OXLOG(@"sceneWillEnterForeground: no DisplayLink");
    return;
  }
  OXLOG(@"sceneWillEnterForeground: resuming DisplayLink");
  self.displayLink.paused = NO;
  [self updateDisplayLinkRange];
  [self updateCameraDrivenDisplayLinkState];
  oxide_host_app_will_enter_foreground();
}

- (void)sceneDidDisconnect:(UIScene *)scene {
  (void)scene;
  OXLOG(@"sceneDidDisconnect: tearing down DisplayLink");
  [self.displayLink invalidate];
  self.displayLink = nil;
  if (gActiveRustSceneDelegate == self) {
    gActiveRustSceneDelegate = nil;
  }
  oxide_cam_set_preview_publish_callback(NULL, NULL);
  self.perfCameraPreviewView.previewLayer.session = nil;
  if (self.perfBenchmarkAVFoundationSession != nil) {
    AVCaptureSession *session = self.perfBenchmarkAVFoundationSession;
    dispatch_queue_t queue = self.perfBenchmarkAVFoundationQueue;
    if (queue != NULL) {
      dispatch_sync(queue, ^{
        [session stopRunning];
      });
    } else {
      [session stopRunning];
    }
    self.perfBenchmarkAVFoundationSession = nil;
    self.perfBenchmarkAVFoundationQueue = NULL;
  }
  if (OxidePerfActualAppCameraBenchmarkEnabled()) {
    CFNotificationCenterRemoveObserver(
        CFNotificationCenterGetDarwinNotifyCenter(),
        (__bridge const void *)(self), kOxidePerfStartNotificationCF, NULL);
    if (self.perfBenchmarkIterationStartNotificationName.length > 0) {
      CFNotificationCenterRemoveObserver(
          CFNotificationCenterGetDarwinNotifyCenter(),
          (__bridge const void *)(self),
          (__bridge CFNotificationName)
              self.perfBenchmarkIterationStartNotificationName,
          NULL);
      self.perfBenchmarkIterationStartNotificationName = nil;
    }
  }
  gHostAppReady = NO;
  StopMetalCapture();
  oxide_host_app_will_terminate();
}

- (BOOL)bindPerfCameraPreviewLayerIfNeeded {
  OxidePerfCameraPreviewView *previewView = self.perfCameraPreviewView;
  if (previewView == nil) {
    return NO;
  }
  if (previewView.previewLayer.session != nil) {
    return YES;
  }
  void *sessionPtr = oxide_cam_get_running_session();
  if (sessionPtr == NULL) {
    return NO;
  }
  AVCaptureSession *session = (__bridge AVCaptureSession *)sessionPtr;
  previewView.previewLayer.session = session;
  AVCaptureConnection *connection = previewView.previewLayer.connection;
  if (connection != nil) {
    connection.automaticallyAdjustsVideoMirroring = NO;
    connection.videoMirrored = NO;
    CGFloat portraitAngle = 90.0;
    if (@available(iOS 17.0, *)) {
      if ([connection isVideoRotationAngleSupported:portraitAngle]) {
        connection.videoRotationAngle = portraitAngle;
      }
    }
  }
  OXLOG(@"bound perf hybrid preview layer to running Oxide camera session");
  return YES;
}

- (void)onTick:(CADisplayLink *)link {
  if (IsRunningUITest() && !IsRunningPerfBenchmarkHost()) {
    return;
  }
  (void)link;
  gAppDebugPerf.on_tick_calls += 1;
  if (OxideHotPathLoggingEnabled()) {
    OXLOG(@"onTick start (main=%d) hostReady=%d", (int)[NSThread isMainThread],
          (int)gHostAppReady);
    UILog(@"tick");
  }
  MetalView *view = (MetalView *)gMetalView;
  if (!view) {
    if (OxideHotPathLoggingEnabled()) {
      OXLOG(@"onTick: no MetalView");
      UILog(@"no MetalView");
    }
    return;
  }
  if (!gHostAppReady) {
    EnsureHostInitialized(view);
  }
  if (OxidePerfActualAppCameraBenchmarkEnabled()) {
    [self pollActualAppCameraBenchmarkReadiness];
  }
  if (!gHostAppReady) {
    if (OxideHotPathLoggingEnabled()) {
      OXLOG(@"onTick: host not ready");
      UILog(@"host not ready");
    }
    return;
  }
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  CGSize size = layer.drawableSize;
  CGFloat scale = view.window.screen.nativeScale;
  double tickT0Ms = OxidePerfNowMs();
  BOOL frameDrivenScheduling = OxidePerfCameraFrameDrivenSchedulingEnabled();
  BOOL cameraTriggeredRender = NO;
  oxide_host_camera_tick_perf_t tickPerf = {0};
  tickPerf.serial = gLastCameraTickPerf.serial + 1;
  tickPerf.drawable_width = (uint32_t)lrintf((float)size.width);
  tickPerf.drawable_height = (uint32_t)lrintf((float)size.height);
  tickPerf.drawable_scale = (float)scale;
  if (frameDrivenScheduling) {
    cameraTriggeredRender =
        atomic_exchange_explicit(&gCameraPreviewNeedsPresent, 0,
                                 memory_order_acq_rel) != 0;
    if (!cameraTriggeredRender) {
      tickPerf.skipped = 1;
      tickPerf.tick_total_ms = (float)(OxidePerfNowMs() - tickT0Ms);
      gLastCameraTickPerf = tickPerf;
      [self updateCameraDrivenDisplayLinkState];
      return;
    }
    gAppDebugPerf.camera_frame_triggered_renders += 1;
  }
  double planT0Ms = OxidePerfNowMs();
  int32_t rc_plan = oxide_host_camera_preview_plan(
      (uint32_t)lrintf((float)size.width), (uint32_t)lrintf((float)size.height),
      (float)scale);
  tickPerf.plan_ms = (float)(OxidePerfNowMs() - planT0Ms);
  int32_t planReason = oxide_host_camera_preview_plan_reason(
      (uint32_t)lrintf((float)size.width), (uint32_t)lrintf((float)size.height),
      (float)scale);
  tickPerf.plan_reason = (uint32_t)planReason;
  if (rc_plan < 0) {
    tickPerf.tick_total_ms = (float)(OxidePerfNowMs() - tickT0Ms);
    gLastCameraTickPerf = tickPerf;
    [self updateCameraDrivenDisplayLinkState];
    OXLOG(@"camera_preview_plan rc=%d", rc_plan);
    UILog([NSString stringWithFormat:@"preview plan rc=%d", rc_plan]);
    return;
  }
  if (OxidePerfCameraRealAppHybridVisiblePreviewEnabled()) {
    tickPerf.skipped = 1;
    tickPerf.tick_total_ms = (float)(OxidePerfNowMs() - tickT0Ms);
    gLastCameraTickPerf = tickPerf;
    [self bindPerfCameraPreviewLayerIfNeeded];
    self.fpsCount += 1;
    CFTimeInterval now = CACurrentMediaTime();
    if (now - self.fpsLastSample >= 0.5) {
      [self refreshStats];
      self.fpsLastSample = now;
      self.fpsCount = 0;
    }
    [self updateCameraDrivenDisplayLinkState];
    return;
  }
  if (rc_plan == 0) {
    gAppDebugPerf.plan_skips += 1;
    tickPerf.skipped = 1;
    tickPerf.tick_total_ms = (float)(OxidePerfNowMs() - tickT0Ms);
    gLastCameraTickPerf = tickPerf;
    [self updateCameraDrivenDisplayLinkState];
    return;
  }
  BOOL noVisiblePresent = OxidePerfCameraNoVisiblePresentEnabled() &&
                          OxidePerfCameraRealAppHostEnabled() &&
                          !OxidePerfCameraRealAppHybridVisiblePreviewEnabled();
  id<CAMetalDrawable> drawable = nil;
  if (!noVisiblePresent) {
    double drawableAcquireT0Ms = OxidePerfNowMs();
    drawable = [layer nextDrawable];
    tickPerf.drawable_acquire_ms =
        (float)(OxidePerfNowMs() - drawableAcquireT0Ms);
    if (!drawable) {
      tickPerf.tick_total_ms = (float)(OxidePerfNowMs() - tickT0Ms);
      gLastCameraTickPerf = tickPerf;
      [self updateCameraDrivenDisplayLinkState];
      OXLOG(@"nextDrawable returned nil");
      return;
    }
    tickPerf.drawable_acquired = 1;
    gAppDebugPerf.drawables_acquired += 1;
    if (OxideHotPathLoggingEnabled()) {
      OXLOG(@"presenting drawable %p (class=%@)", (__bridge void *)drawable,
            [drawable class]);
      UILog(
          [NSString stringWithFormat:@"present %p", (__bridge void *)drawable]);
    }
  }
  double frameCallT0Ms = OxidePerfNowMs();
  int32_t rc_frame = oxide_host_app_frame_with_drawable(
      (uint32_t)lrintf((float)size.width), (uint32_t)lrintf((float)size.height),
      (float)scale, drawable != nil ? (__bridge void *)drawable : NULL);
  tickPerf.frame_call_ms = (float)(OxidePerfNowMs() - frameCallT0Ms);
  tickPerf.frame_submitted = (uint8_t)(rc_frame == 0);
  if (rc_frame == 0) {
    gAppDebugPerf.command_buffers_committed += 1;
  }
  tickPerf.tick_total_ms = (float)(OxidePerfNowMs() - tickT0Ms);
  gLastCameraTickPerf = tickPerf;
  [self updateCameraDrivenDisplayLinkState];
  if (rc_frame != 0) {
    OXLOG(@"app_frame rc=%d", rc_frame);
    UILog([NSString stringWithFormat:@"frame rc=%d", rc_frame]);
  }
  self.fpsCount += 1;
  CFTimeInterval now = CACurrentMediaTime();
  if (now - self.fpsLastSample >= 0.5) {
    [self refreshStats];
    self.fpsLastSample = now;
    self.fpsCount = 0;
  }
}

@end

@interface OxideApplication : UIApplication
@end

@implementation OxideApplication
- (void)sendEvent:(UIEvent *)event {
  if (OxideTouchDebugEnabled()) {
    UIWindow *window = ResolveWindow(gMetalView);
    UIView *view = gMetalView ?: window.rootViewController.view ?: window;
    OXLOG(@"touch-debug application sendEvent %@",
          OxideEventSummary(event, view));
  }
  [super sendEvent:event];
}
@end

@interface RustAppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation RustAppDelegate
- (BOOL)application:(UIApplication *)application
    didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
  (void)application;
  (void)launchOptions;
  oxide_host_push_bootstrap();
  return YES;
}

- (UISceneConfiguration *)application:(UIApplication *)application
    configurationForConnectingSceneSession:
        (UISceneSession *)connectingSceneSession
                                   options:(UISceneConnectionOptions *)options {
  (void)application;
  (void)options;
  UISceneConfiguration *config =
      [UISceneConfiguration configurationWithName:@"RustScene"
                                      sessionRole:connectingSceneSession.role];
  config.delegateClass = [RustSceneDelegate class];
  return config;
}

- (void)application:(UIApplication *)application
    didDiscardSceneSessions:(NSSet<UISceneSession *> *)sceneSessions {
  (void)application;
  (void)sceneSessions;
}

- (void)applicationDidReceiveMemoryWarning:(UIApplication *)application {
  (void)application;
  oxide_host_on_memory_warning();
}

- (void)applicationWillTerminate:(UIApplication *)application {
  (void)application;
  oxide_host_app_will_terminate();
}

- (void)application:(UIApplication *)application
    didRegisterForRemoteNotificationsWithDeviceToken:(NSData *)deviceToken {
  (void)application;
  oxide_host_push_application_did_register(deviceToken);
}

- (void)application:(UIApplication *)application
    didFailToRegisterForRemoteNotificationsWithError:(NSError *)error {
  (void)application;
  oxide_host_push_application_did_fail(error);
}

- (void)application:(UIApplication *)application
    didReceiveRemoteNotification:(NSDictionary *)userInfo
          fetchCompletionHandler:
              (void (^)(UIBackgroundFetchResult result))completionHandler {
  (void)application;
  oxide_host_push_application_did_receive(userInfo);
  if (completionHandler != nil) {
    completionHandler(UIBackgroundFetchResultNoData);
  }
}

@end

int32_t oxide_host_start(int argc, char **argv) {
  @autoreleasepool {
    OXLOG(@"oxide_host_start: UIApplicationMain begin");
    char *fallback_argv[] = {(char *)"oxide-host", NULL};
    int launch_argc = (argc > 0 && argv != NULL) ? argc : 1;
    char **launch_argv = (argc > 0 && argv != NULL) ? argv : fallback_argv;
    int ret = UIApplicationMain(launch_argc, launch_argv,
                                NSStringFromClass([OxideApplication class]),
                                NSStringFromClass([RustAppDelegate class]));
    OXLOG(@"UIApplicationMain returned: %d", ret);
    return ret;
  }
}
static void UILog(NSString *line) {
  if (OxideTouchDebugEnabled() && !OxideTouchScreenLogEnabled()) {
    return;
  }
  dispatch_on_main(^{
    if (!gUILogLabel || !gUILogLabel.superview) {
      return;
    }
    NSString *prev = gUILogLabel.text ?: @"";
    NSString *next =
        (prev.length > 0) ? [prev stringByAppendingFormat:@"\n%@", line] : line;
    if (next.length > 8000) {
      next = [next substringFromIndex:(next.length - 8000)];
    }
    gUILogLabel.text = next;
  });
}
// Bridge for Rust logs
void oxide_host_ios_log(const char *utf8, size_t len) {
  if (!utf8 || len == 0) {
    return;
  }
  NSString *s = [[NSString alloc] initWithBytes:utf8
                                         length:len
                                       encoding:NSUTF8StringEncoding];
  if (!s) {
    s = [NSString stringWithUTF8String:utf8];
  }
  if (!s) {
    return;
  }
  NSLog(@"[Oxide-Rust] %@", s);
  OxideTouchFileLog([NSString stringWithFormat:@"rust %@", s]);
  UILog(s);
}
