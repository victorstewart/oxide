#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CVPixelBufferIOSurface.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <IOSurface/IOSurfaceRef.h>
#import <Metal/Metal.h>
#import <UIKit/UIKit.h>
#import <os/lock.h>
#import <os/log.h>
#import <os/signpost.h>

#include <dispatch/dispatch.h>
#include <limits.h>
#include <mach/mach_time.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static os_log_t NametagPerfSignpostLog(void) {
  static os_log_t log;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    log = os_log_create("com.oxide.perf", "pointsOfInterest");
  });
  return log;
}

#define NAMETAG_PERF_BEGIN(var, literal)                                       \
  os_signpost_id_t var = OS_SIGNPOST_ID_NULL;                                  \
  if (@available(iOS 12.0, *)) {                                               \
    os_log_t _nametag_perf_log = NametagPerfSignpostLog();                     \
    var = os_signpost_id_generate(_nametag_perf_log);                          \
    os_signpost_interval_begin(_nametag_perf_log, var, literal);               \
  }

#define NAMETAG_PERF_END(var, literal)                                         \
  if (var != OS_SIGNPOST_ID_NULL) {                                            \
    if (@available(iOS 12.0, *)) {                                             \
      os_signpost_interval_end(NametagPerfSignpostLog(), var, literal);        \
    }                                                                          \
  }

static uint64_t NametagPerfNowTicks(void) { return mach_absolute_time(); }

static double NametagPerfElapsedMs(uint64_t start_ticks) {
  static mach_timebase_info_data_t info;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    mach_timebase_info(&info);
  });
  uint64_t elapsed = mach_absolute_time() - start_ticks;
  if (info.denom == 0) {
    return 0.0;
  }
  double nanos = ((double)elapsed * (double)info.numer) / (double)info.denom;
  return nanos / 1000000.0;
}

typedef NS_ENUM(NSUInteger, NametagPerfCameraCaptureContractMode) {
  NametagPerfCameraCaptureContractInputPriority = 0,
  NametagPerfCameraCaptureContractPreset720p = 1,
};

static NametagPerfCameraCaptureContractMode
NametagPerfCameraCaptureContractModeCurrent(void) {
  static NametagPerfCameraCaptureContractMode mode =
      NametagPerfCameraCaptureContractInputPriority;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE"];
    NSString *normalized = [[env ?: @""
        stringByTrimmingCharactersInSet:[NSCharacterSet
                                            whitespaceAndNewlineCharacterSet]]
        lowercaseString];
    if ([normalized isEqualToString:@"preset-720p"] ||
        [normalized isEqualToString:@"preset720p"] ||
        [normalized isEqualToString:@"hd1280x720"]) {
      mode = NametagPerfCameraCaptureContractPreset720p;
    }
  });
  return mode;
}

static BOOL NametagPerfParkedModeCurrent(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_PARKED"];
    NSString *normalized = [[env ?: @""
        stringByTrimmingCharactersInSet:[NSCharacterSet
                                            whitespaceAndNewlineCharacterSet]]
        lowercaseString];
    enabled = [normalized isEqualToString:@"1"] ||
              [normalized isEqualToString:@"true"] ||
              [normalized isEqualToString:@"yes"];
  });
  return enabled;
}

// ==== Shared FFI structures ==================================================

typedef uintptr_t StreamHandle;
typedef uintptr_t RecordingHandle;

struct NametagHostCameraConfig {
  uint32_t width;
  uint32_t height;
  uint32_t fps;
  int32_t device;               // 0 = front, 1 = back
  int32_t capture_mode;         // 0 preview, 1 photo, 2 video (reserved)
  int32_t preview_pixel_format; // 0 = NV12, 1 = BGRA
  bool enable_audio;
};

struct NametagHostCameraStreamFrame {
  uint32_t width;
  uint32_t height;
  uint32_t bytes_per_row;
  uint64_t timestamp_ns;
  const uint8_t *data;
  size_t len;
};

struct NametagHostAudioSample {
  uint32_t channels;
  uint32_t sample_rate_hz;
  uint64_t timestamp_ns;
  const int16_t *data;
  size_t sample_count;
};

struct NametagRecordingOptions {
  const char *output_path;
  uint32_t width;
  uint32_t height;
  uint32_t fps;
  uint32_t bitrate;
  int32_t container; // 0=mp4, 1=mov
  bool include_audio;
};

struct NametagRecordingEvent {
  int32_t kind; // 0=Completed, 1=Cancelled, 2=Failed
  const char *path;
  size_t path_len;
  uint64_t duration_ns;
  uint64_t size_bytes;
  bool had_audio;
  const char *error;
  size_t error_len;
};

struct NametagPhotoOptions {
  bool high_speed_from_preview;
  int32_t flash_mode; // 0=Off, 1=On, 2=Auto
};

struct NametagPhotoFrame {
  uint32_t width;
  uint32_t height;
  uint32_t y_stride;
  uint32_t uv_stride;
  uint8_t bit_depth;
  uint8_t matrix;
  uint8_t video_range;
  uint16_t rotation_deg;
  uint64_t timestamp_ns;
  const uint8_t *data;
  size_t len;
};

struct NametagPhotoEvent {
  int32_t kind; // 0=Completed, 1=Failed
  struct NametagPhotoFrame frame;
  int32_t error_code;
  const char *error;
  size_t error_len;
};

struct OxideCamPerfSnapshot {
  float capture_total_ms;
  float capture_sample_setup_ms;
  float capture_lock_ms;
  float capture_texture_bridge_ms;
  float capture_publish_ms;
  float capture_publish_lock_ms;
  float capture_publish_texture_refs_ms;
  float capture_publish_pixel_buffer_ms;
  float capture_frame_delivery_ms;
  uint64_t sample_delivery_pool_bytes;
  uint32_t sample_delivery_pool_surfaces;
  uint64_t active_sample_surface_bytes;
  uint32_t active_sample_surface_surfaces;
  uint32_t active_sample_buffers;
  uint64_t peak_active_sample_surface_bytes;
  uint32_t peak_active_sample_surface_surfaces;
  uint32_t peak_active_sample_buffers;
  uint32_t sample_delivery_total_samples;
  uint32_t sample_delivery_reused_frames;
  uint32_t sample_delivery_reused_surfaces;
  uint32_t sample_delivery_max_reuse_gap_frames;
  uint64_t retained_sample_surface_bytes;
  uint32_t retained_sample_surface_surfaces;
  uint64_t retained_published_slot_surface_bytes;
  uint32_t retained_published_slot_surfaces;
  uint64_t retained_latest_pixel_buffer_surface_bytes;
  uint32_t retained_latest_pixel_buffer_surface_surfaces;
  uint64_t latest_published_generation;
  uint64_t latest_published_timestamp_ns;
  uint64_t latest_presented_generation;
  uint32_t generation_advances;
  uint32_t samples_received;
  uint32_t samples_dropped_prebridge;
  uint32_t samples_bridged;
  uint32_t samples_published;
  uint32_t samples_presented;
  uint32_t samples_superseded_before_present;
};

struct OxideCamContractSnapshot {
  uint32_t active_width;
  uint32_t active_height;
  float active_fps;
  uint32_t video_range;
  uint32_t color_space;
};

struct OxideCamAcquiredFrame {
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
};

typedef void (*NametagFrameCallback)(
    const struct NametagHostCameraStreamFrame *, void *ctx);
typedef void (*NametagAudioCallback)(const struct NametagHostAudioSample *,
                                     void *ctx);
typedef void (*NametagRecordingCallback)(const struct NametagRecordingEvent *,
                                         void *ctx);
typedef void (*NametagPhotoCallback)(const struct NametagPhotoEvent *,
                                     void *ctx);

struct OxideCamFrame {
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
};

struct OxideCamAudio {
  const int16_t *audio_ptr;
  size_t sample_count;
  uint32_t channels;
  uint32_t sample_rate_hz;
  uint64_t timestamp_ns;
};

struct OxideCamRecordEvent {
  uint32_t kind;
  const uint8_t *path_ptr;
  size_t path_len;
  uint64_t duration_ns;
  uint64_t size_bytes;
  uint8_t had_audio;
  int32_t error_code;
  const uint8_t *error_msg_ptr;
  size_t error_msg_len;
};

struct OxideCamPhotoEvent {
  uint32_t kind;
  struct OxideCamFrame frame;
  int32_t error_code;
  const uint8_t *error_msg_ptr;
  size_t error_msg_len;
};

typedef void (*OxideCameraFrameCallback)(const struct OxideCamFrame *);
typedef void (*OxideCameraAudioCallback)(const struct OxideCamAudio *);
typedef void (*OxideCameraRecordCallback)(const struct OxideCamRecordEvent *);
typedef void (*OxideCameraPhotoCallback)(const struct OxideCamPhotoEvent *);
typedef void (*OxideCameraPreviewPublishCallback)(uint64_t generation,
                                                  uint64_t timestamp_ns,
                                                  void *ctx);

static OxideCameraFrameCallback g_oxide_camera_frame_callback = NULL;
static OxideCameraAudioCallback g_oxide_camera_audio_callback = NULL;
static OxideCameraRecordCallback g_oxide_camera_record_callback = NULL;
static OxideCameraPhotoCallback g_oxide_camera_photo_callback = NULL;
static OxideCameraPreviewPublishCallback
    g_oxide_camera_preview_publish_callback = NULL;
static void *g_oxide_camera_preview_publish_context = NULL;

enum { kOxideCameraPublishedSlotCount = 4 };

static uint32_t NametagPerfPreviewPublishedSlotCountCurrent(void) {
  static uint32_t count = kOxideCameraPublishedSlotCount;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_PREVIEW_PUBLISHED_SLOT_COUNT"];
    NSInteger parsed = [[env ?: @""
        stringByTrimmingCharactersInSet:[NSCharacterSet
                                            whitespaceAndNewlineCharacterSet]]
        integerValue];
    if (parsed >= 1 && parsed <= kOxideCameraPublishedSlotCount) {
      count = (uint32_t)parsed;
    }
  });
  return count;
}

static BOOL NametagPerfPreviewPrebridgeDropEnabled(void) {
  static BOOL enabled = NO;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *env = [NSProcessInfo.processInfo.environment
        objectForKey:@"OXIDE_PERF_CAMERA_PREBRIDGE_DROP"];
    NSString *normalized = [[env ?: @""
        stringByTrimmingCharactersInSet:[NSCharacterSet
                                            whitespaceAndNewlineCharacterSet]]
        lowercaseString];
    enabled = [normalized isEqualToString:@"1"] ||
              [normalized isEqualToString:@"true"] ||
              [normalized isEqualToString:@"yes"];
  });
  return enabled;
}

static inline void OxideDispatchPreviewPublishCallback(uint64_t generation,
                                                       uint64_t timestamp_ns) {
  OxideCameraPreviewPublishCallback callback =
      g_oxide_camera_preview_publish_callback;
  if (callback != NULL) {
    callback(generation, timestamp_ns, g_oxide_camera_preview_publish_context);
  }
}

static inline uint64_t OxidePackPublishedFrameState(uint32_t slot,
                                                    uint64_t generation) {
  return ((generation & 0x00FFFFFFFFFFFFFFull) << 8) | (uint64_t)(slot & 0xFFu);
}

static inline uint32_t OxidePublishedFrameSlot(uint64_t state) {
  return (uint32_t)(state & 0xFFu);
}

static inline uint64_t OxidePublishedFrameGeneration(uint64_t state) {
  return state >> 8;
}

static void
oxide_camera_stream_callback(const struct NametagHostCameraStreamFrame *frame,
                             void *ctx);
static void
oxide_camera_audio_callback(const struct NametagHostAudioSample *sample,
                            void *ctx);

int32_t oxide_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w,
                                int32_t *h, int32_t *bitdepth, int32_t *matrix,
                                int32_t *video_range, int32_t *colorspace);
int32_t
oxide_cam_acquire_latest_frame_ex(uint64_t min_generation_exclusive,
                                  struct OxideCamAcquiredFrame *out_frame);
void *oxide_cam_get_running_session(void);
uint64_t oxide_cam_peek_latest_generation(void);
uint64_t oxide_cam_peek_latest_timestamp_ns(void);
void oxide_cam_release_acquired(uint32_t slot, uint64_t generation);

static uint64_t timestamp_ns_from_sample(CMSampleBufferRef sample) {
  CMTime pts = CMSampleBufferGetPresentationTimeStamp(sample);
  if (pts.timescale == 0) {
    return 0;
  }
  CMTime ns =
      CMTimeConvertScale(pts, 1000000000, kCMTimeRoundingMethod_Default);
  if (ns.timescale == 0) {
    return 0;
  }
  return (uint64_t)ns.value;
}

static uint64_t timestamp_ns_from_time(CMTime time) {
  if (!CMTIME_IS_VALID(time) || time.timescale == 0) {
    return 0;
  }
  CMTime ns =
      CMTimeConvertScale(time, 1000000000, kCMTimeRoundingMethod_Default);
  if (!CMTIME_IS_VALID(ns) || ns.timescale == 0) {
    return 0;
  }
  return (uint64_t)MAX((int64_t)0, ns.value);
}

static id<MTLDevice> SharedMetalDevice(void) {
  static id<MTLDevice> device = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    device = MTLCreateSystemDefaultDevice();
  });
  return device;
}

static void NametagDispatchSyncMainQueue(dispatch_block_t block) {
  if (block == nil) {
    return;
  }
  if ([NSThread isMainThread]) {
    block();
    return;
  }
  dispatch_sync(dispatch_get_main_queue(), block);
}

static void *kNametagCameraSessionQueueKey = &kNametagCameraSessionQueueKey;

static void NametagDispatchBlockingQueue(dispatch_queue_t queue, void *key,
                                         dispatch_block_t block) {
  if (queue == nil || block == nil) {
    return;
  }
  if (key != NULL && dispatch_get_specific(key) != NULL) {
    block();
    return;
  }
  dispatch_semaphore_t finished = dispatch_semaphore_create(0);
  dispatch_async(queue, ^{
    block();
    dispatch_semaphore_signal(finished);
  });
  dispatch_semaphore_wait(finished, DISPATCH_TIME_FOREVER);
}

static dispatch_queue_t CameraRegistryQueue(void) {
  static dispatch_queue_t queue;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    queue = dispatch_queue_create("com.nametag.camera.registry",
                                  DISPATCH_QUEUE_SERIAL);
  });
  return queue;
}

static NSMutableDictionary<NSValue *, id> *CameraRegistry(void) {
  static NSMutableDictionary<NSValue *, id> *registry;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    registry = [[NSMutableDictionary alloc] init];
  });
  return registry;
}

static NSString *const kNametagSampleBufferLifetimeAttachmentKey =
    @"com.oxide.camera.sample-buffer-lifetime";

@interface NametagCameraPublishedSlot : NSObject
@property(nonatomic) CVMetalTextureRef yRef;
@property(nonatomic) CVMetalTextureRef uvRef;
@property(nonatomic, strong) id<MTLTexture> yTex;
@property(nonatomic, strong) id<MTLTexture> uvTex;
@property(nonatomic, strong) NSValue *sampleSurfaceKey;
@property(nonatomic, assign) uint64_t sampleSurfaceBytes;
@property(nonatomic, assign) int width;
@property(nonatomic, assign) int height;
@property(nonatomic, assign) int bitDepth;
@property(nonatomic, assign) int matrix;
@property(nonatomic, assign) int videoRange;
@property(nonatomic, assign) int colorSpace;
@property(nonatomic, assign) uint64_t generation;
@property(nonatomic, assign) uint64_t timestampNs;
@end

@implementation NametagCameraPublishedSlot
- (void)dealloc {
  if (_yRef != NULL) {
    CFRelease(_yRef);
    _yRef = NULL;
  }
  if (_uvRef != NULL) {
    CFRelease(_uvRef);
    _uvRef = NULL;
  }
}
@end

@class NametagCameraStream;

@interface NametagCameraSampleLifetimeToken : NSObject
@property(nonatomic, weak) NametagCameraStream *stream;
@property(nonatomic, strong) NSValue *sampleSurfaceKey;
@property(nonatomic, assign) uint64_t sampleSurfaceBytes;
@end

static NSValue *CameraKeyFromPointer(void *ptr) {
  return [NSValue valueWithPointer:ptr];
}

static int
nametag_photo_matrix_from_pixel_buffer(CVPixelBufferRef pixelBuffer) {
  int matrix = 0;
  CFTypeRef attachment =
      CVBufferCopyAttachment(pixelBuffer, kCVImageBufferYCbCrMatrixKey, NULL);
  if (attachment) {
    if (CFGetTypeID(attachment) == CFStringGetTypeID()) {
      CFStringRef value = (CFStringRef)attachment;
      if (CFEqual(value, kCVImageBufferYCbCrMatrix_ITU_R_601_4)) {
        matrix = 1;
      } else if (CFEqual(value, kCVImageBufferYCbCrMatrix_ITU_R_2020)) {
        matrix = 2;
      }
    }
    CFRelease(attachment);
  }
  return matrix;
}

static BOOL nametag_prepare_texture_cache(CVMetalTextureCacheRef *cache) {
  if (cache == NULL) {
    return NO;
  }
  if (*cache != NULL) {
    return YES;
  }
  id<MTLDevice> device = SharedMetalDevice();
  if (!device) {
    return NO;
  }
  CVReturn rc =
      CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, device, NULL, cache);
  return rc == kCVReturnSuccess;
}

static BOOL nametag_copy_nv12_textures_from_pixel_buffer(
    CVPixelBufferRef pixelBuffer, CVMetalTextureCacheRef textureCache,
    CVMetalTextureRef *outYRef, CVMetalTextureRef *outUVRef, int *outBitDepth,
    int *outMatrix, int *outVideoRange, int *outWidth, int *outHeight) {
  if (pixelBuffer == NULL || textureCache == NULL ||
      !CVPixelBufferIsPlanar(pixelBuffer) ||
      CVPixelBufferGetPlaneCount(pixelBuffer) < 2) {
    return NO;
  }

  OSType pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer);
  int bitDepth = 8;
  int videoRange = 0;
  switch (pixelFormat) {
  case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
    bitDepth = 8;
    videoRange = 0;
    break;
  case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
    bitDepth = 8;
    videoRange = 1;
    break;
  case kCVPixelFormatType_420YpCbCr10BiPlanarFullRange:
    bitDepth = 10;
    videoRange = 0;
    break;
  case kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange:
    bitDepth = 10;
    videoRange = 1;
    break;
  default:
    return NO;
  }

  size_t widthY = CVPixelBufferGetWidthOfPlane(pixelBuffer, 0);
  size_t heightY = CVPixelBufferGetHeightOfPlane(pixelBuffer, 0);
  size_t widthUV = CVPixelBufferGetWidthOfPlane(pixelBuffer, 1);
  size_t heightUV = CVPixelBufferGetHeightOfPlane(pixelBuffer, 1);
  if (widthY == 0 || heightY == 0 || widthUV == 0 || heightUV == 0) {
    return NO;
  }

  MTLPixelFormat yFormat =
      (bitDepth == 10) ? MTLPixelFormatR16Unorm : MTLPixelFormatR8Unorm;
  MTLPixelFormat uvFormat =
      (bitDepth == 10) ? MTLPixelFormatRG16Unorm : MTLPixelFormatRG8Unorm;
  CVMetalTextureRef yRef = NULL;
  CVMetalTextureRef uvRef = NULL;
  CVReturn yRc = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, textureCache, pixelBuffer, NULL, yFormat, widthY,
      heightY, 0, &yRef);
  CVReturn uvRc = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, textureCache, pixelBuffer, NULL, uvFormat, widthUV,
      heightUV, 1, &uvRef);
  if (yRc != kCVReturnSuccess || uvRc != kCVReturnSuccess || yRef == NULL ||
      uvRef == NULL) {
    if (yRef) {
      CFRelease(yRef);
    }
    if (uvRef) {
      CFRelease(uvRef);
    }
    return NO;
  }

  if (outBitDepth) {
    *outBitDepth = bitDepth;
  }
  if (outMatrix) {
    *outMatrix = nametag_photo_matrix_from_pixel_buffer(pixelBuffer);
  }
  if (outVideoRange) {
    *outVideoRange = videoRange;
  }
  if (outWidth) {
    *outWidth = (int)widthY;
  }
  if (outHeight) {
    *outHeight = (int)heightY;
  }
  if (outYRef) {
    *outYRef = yRef;
  } else {
    CFRelease(yRef);
  }
  if (outUVRef) {
    *outUVRef = uvRef;
  } else {
    CFRelease(uvRef);
  }
  return YES;
}

static BOOL nametag_copy_bgra_texture_from_pixel_buffer(
    CVPixelBufferRef pixelBuffer, CVMetalTextureCacheRef textureCache,
    CVMetalTextureRef *outTextureRef, int *outWidth, int *outHeight) {
  if (pixelBuffer == NULL || textureCache == NULL || outTextureRef == NULL) {
    return NO;
  }

  OSType pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer);
  if (pixelFormat != kCVPixelFormatType_32BGRA) {
    return NO;
  }

  size_t width = CVPixelBufferGetWidth(pixelBuffer);
  size_t height = CVPixelBufferGetHeight(pixelBuffer);
  if (width == 0 || height == 0) {
    return NO;
  }

  CVMetalTextureRef textureRef = NULL;
  CVReturn rc = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, textureCache, pixelBuffer, NULL,
      MTLPixelFormatBGRA8Unorm, width, height, 0, &textureRef);
  if (rc != kCVReturnSuccess || textureRef == NULL) {
    return NO;
  }

  if (outWidth) {
    *outWidth = (int)width;
  }
  if (outHeight) {
    *outHeight = (int)height;
  }
  *outTextureRef = textureRef;
  return YES;
}

static BOOL nametag_photo_frame_from_pixel_buffer(
    CVPixelBufferRef pixelBuffer, uint64_t timestampNs, uint16_t rotationDeg,
    struct NametagPhotoFrame *outFrame, NSString **outError) {
  if (outFrame == NULL) {
    if (outError) {
      *outError = @"photo capture frame output missing";
    }
    return NO;
  }
  memset(outFrame, 0, sizeof(*outFrame));
  if (pixelBuffer == NULL) {
    if (outError) {
      *outError = @"photo capture pixel buffer unavailable";
    }
    return NO;
  }
  if (!CVPixelBufferIsPlanar(pixelBuffer) ||
      CVPixelBufferGetPlaneCount(pixelBuffer) < 2) {
    if (outError) {
      *outError = @"photo capture pixel buffer is not NV12";
    }
    return NO;
  }

  OSType pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer);
  switch (pixelFormat) {
  case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
  case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
  case kCVPixelFormatType_420YpCbCr10BiPlanarFullRange:
  case kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange:
    break;
  default:
    if (outError) {
      *outError =
          [NSString stringWithFormat:@"unsupported photo pixel format %u",
                                     (unsigned int)pixelFormat];
    }
    return NO;
  }

  CVPixelBufferLockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
  const size_t widthY = CVPixelBufferGetWidthOfPlane(pixelBuffer, 0);
  const size_t heightY = CVPixelBufferGetHeightOfPlane(pixelBuffer, 0);
  const size_t bytesPerRowY =
      CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0);
  const size_t bytesPerRowUV =
      CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 1);
  const size_t heightUV = CVPixelBufferGetHeightOfPlane(pixelBuffer, 1);
  const uint8_t *baseY = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0);
  const uint8_t *baseUV = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 1);

  if (widthY == 0 || heightY == 0 || bytesPerRowY == 0 || bytesPerRowUV == 0 ||
      baseY == NULL || baseUV == NULL) {
    CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
    if (outError) {
      *outError = @"photo capture pixel buffer planes unavailable";
    }
    return NO;
  }

  const size_t yLen = bytesPerRowY * heightY;
  const size_t uvLen = bytesPerRowUV * heightUV;
  const size_t totalLen = yLen + uvLen;
  uint8_t *buffer = (uint8_t *)malloc(totalLen);
  if (buffer == NULL) {
    CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
    if (outError) {
      *outError = @"photo capture buffer allocation failed";
    }
    return NO;
  }

  for (size_t row = 0; row < heightY; row++) {
    memcpy(buffer + row * bytesPerRowY, baseY + row * bytesPerRowY,
           bytesPerRowY);
  }
  uint8_t *uvDest = buffer + yLen;
  for (size_t row = 0; row < heightUV; row++) {
    memcpy(uvDest + row * bytesPerRowUV, baseUV + row * bytesPerRowUV,
           bytesPerRowUV);
  }
  CVPixelBufferUnlockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);

  outFrame->width = (uint32_t)widthY;
  outFrame->height = (uint32_t)heightY;
  outFrame->y_stride = (uint32_t)bytesPerRowY;
  outFrame->uv_stride = (uint32_t)bytesPerRowUV;
  outFrame->bit_depth =
      (pixelFormat == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange ||
       pixelFormat == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange)
          ? 10
          : 8;
  outFrame->matrix =
      (uint8_t)nametag_photo_matrix_from_pixel_buffer(pixelBuffer);
  outFrame->video_range =
      (pixelFormat == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange ||
       pixelFormat == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange)
          ? 1
          : 0;
  outFrame->rotation_deg = rotationDeg;
  outFrame->timestamp_ns = timestampNs;
  outFrame->data = buffer;
  outFrame->len = totalLen;
  return YES;
}

static void nametag_photo_frame_release(struct NametagPhotoFrame *frame) {
  if (frame == NULL || frame->data == NULL) {
    return;
  }
  free((void *)frame->data);
  frame->data = NULL;
  frame->len = 0;
}

static void nametag_dispatch_photo_failure(NametagPhotoCallback callback,
                                           void *context, int32_t errorCode,
                                           NSString *message) {
  if (callback == NULL) {
    return;
  }
  NSString *resolved = message ?: @"photo capture failed";
  struct NametagPhotoEvent event = {
      .kind = 1,
      .frame = {0},
      .error_code = errorCode,
      .error = [resolved UTF8String],
      .error_len = [resolved lengthOfBytesUsingEncoding:NSUTF8StringEncoding]};
  callback(&event, context);
}

static void
nametag_dispatch_photo_completed(NametagPhotoCallback callback, void *context,
                                 const struct NametagPhotoFrame *frame) {
  if (callback == NULL || frame == NULL) {
    return;
  }
  struct NametagPhotoEvent event = {.kind = 0,
                                    .frame = *frame,
                                    .error_code = 0,
                                    .error = NULL,
                                    .error_len = 0};
  callback(&event, context);
}

// ==== Recorder
// ================================================================

@interface NametagCameraRecorder : NSObject
- (instancetype)initWithOptions:(const struct NametagRecordingOptions *)options
                       callback:(NametagRecordingCallback)callback
                        context:(void *)context;
- (void)appendVideoSample:(CMSampleBufferRef)sample;
- (void)appendAudioSample:(CMSampleBufferRef)sample;
- (void)finish;
- (void)cancel;
@end

@implementation NametagCameraRecorder {
  AVAssetWriter *_writer;
  AVAssetWriterInput *_videoInput;
  AVAssetWriterInput *_audioInput;
  NametagRecordingCallback _callback;
  void *_context;
  NSString *_path;
  BOOL _isRecording;
  BOOL _hasAudio;
  uint64_t _startTimeNs;
  dispatch_queue_t _queue;
}

- (instancetype)initWithOptions:(const struct NametagRecordingOptions *)options
                       callback:(NametagRecordingCallback)callback
                        context:(void *)context {
  self = [super init];
  if (!self)
    return nil;

  _callback = callback;
  _context = context;
  _path = [NSString stringWithUTF8String:options->output_path];
  _hasAudio = options->include_audio;
  _queue = dispatch_queue_create("com.nametag.camera.recorder",
                                 DISPATCH_QUEUE_SERIAL);

  NSError *error = nil;
  NSURL *url = [NSURL fileURLWithPath:_path];
  [[NSFileManager defaultManager] removeItemAtURL:url error:nil];

  AVFileType fileType = AVFileTypeMPEG4;
  if (options->container == 1) {
    fileType = AVFileTypeQuickTimeMovie;
  }

  _writer = [AVAssetWriter assetWriterWithURL:url
                                     fileType:fileType
                                        error:&error];
  if (error) {
    NSLog(@"Failed to create writer: %@", error);
    return nil;
  }

  NSDictionary *videoSettings = @{
    AVVideoCodecKey : AVVideoCodecTypeHEVC,
    AVVideoWidthKey : @(options->width),
    AVVideoHeightKey : @(options->height),
    AVVideoCompressionPropertiesKey : @{
      AVVideoAverageBitRateKey :
          @(options->bitrate > 0 ? options->bitrate : 5000000)
    }
  };

  _videoInput =
      [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeVideo
                                         outputSettings:videoSettings];
  _videoInput.expectsMediaDataInRealTime = YES;
  if ([_writer canAddInput:_videoInput]) {
    [_writer addInput:_videoInput];
  } else {
    NSLog(@"Failed to add video input");
    return nil;
  }

  if (_hasAudio) {
    NSDictionary *audioSettings = @{
      AVFormatIDKey : @(kAudioFormatMPEG4AAC),
      AVNumberOfChannelsKey : @(1),
      AVSampleRateKey : @(44100),
      AVEncoderBitRateKey : @(64000)
    };
    _audioInput =
        [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeAudio
                                           outputSettings:audioSettings];
    _audioInput.expectsMediaDataInRealTime = YES;
    if ([_writer canAddInput:_audioInput]) {
      [_writer addInput:_audioInput];
    }
  }

  if (![_writer startWriting]) {
    NSLog(@"Failed to start writing: %@", _writer.error);
    return nil;
  }
  _isRecording = YES;

  return self;
}

- (void)appendVideoSample:(CMSampleBufferRef)sample {
  dispatch_async(_queue, ^{
    if (!self->_isRecording || !self->_videoInput.isReadyForMoreMediaData)
      return;

    // Adjust timestamp if needed, for now just append
    // In a real app we might need to offset timestamps to 0
    if (self->_startTimeNs == 0) {
      CMTime pts = CMSampleBufferGetPresentationTimeStamp(sample);
      [self->_writer startSessionAtSourceTime:pts];
      self->_startTimeNs = (uint64_t)(CMTimeGetSeconds(pts) * 1e9);
    }

    [self->_videoInput appendSampleBuffer:sample];
  });
}

- (void)appendAudioSample:(CMSampleBufferRef)sample {
  dispatch_async(_queue, ^{
    if (!self->_isRecording || self->_startTimeNs == 0 ||
        !self->_audioInput.isReadyForMoreMediaData)
      return;
    [self->_audioInput appendSampleBuffer:sample];
  });
}

- (void)finish {
  dispatch_async(_queue, ^{
    if (!self->_isRecording)
      return;
    self->_isRecording = NO;
    [self->_videoInput markAsFinished];
    if (self->_audioInput)
      [self->_audioInput markAsFinished];

    [self->_writer finishWritingWithCompletionHandler:^{
      if (self->_writer.status != AVAssetWriterStatusCompleted) {
        NSString *message =
            self->_writer.error.localizedDescription ?: @"recording failed";
        struct NametagRecordingEvent event = {
            .kind = 2,
            .path = [self->_path UTF8String],
            .path_len =
                [self->_path lengthOfBytesUsingEncoding:NSUTF8StringEncoding],
            .duration_ns = 0,
            .size_bytes = 0,
            .had_audio = self->_hasAudio,
            .error = [message UTF8String],
            .error_len =
                [message lengthOfBytesUsingEncoding:NSUTF8StringEncoding]};
        self->_callback(&event, self->_context);
        return;
      }
      uint64_t durationNs = 0;
      AVURLAsset *asset =
          [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:self->_path]
                              options:nil];
      CMTime duration = asset.duration;
      if (CMTIME_IS_NUMERIC(duration) && duration.timescale != 0) {
        CMTime ns = CMTimeConvertScale(duration, 1000000000,
                                       kCMTimeRoundingMethod_Default);
        if (ns.value > 0) {
          durationNs = (uint64_t)ns.value;
        }
      }

      struct NametagRecordingEvent event = {
          .kind = 0, // Completed
          .path = [self->_path UTF8String],
          .path_len =
              [self->_path lengthOfBytesUsingEncoding:NSUTF8StringEncoding],
          .duration_ns = durationNs,
          .size_bytes = 0,
          .had_audio = self->_hasAudio,
          .error = NULL,
          .error_len = 0};

      // Get file size
      NSError *attrError = nil;
      NSDictionary *attrs =
          [[NSFileManager defaultManager] attributesOfItemAtPath:self->_path
                                                           error:&attrError];
      if (attrs) {
        event.size_bytes = [attrs fileSize];
      }

      self->_callback(&event, self->_context);
    }];
  });
}

- (void)cancel {
  dispatch_async(_queue, ^{
    if (!self->_isRecording)
      return;
    self->_isRecording = NO;
    [self->_writer cancelWriting];
    [[NSFileManager defaultManager] removeItemAtPath:self->_path error:nil];

    struct NametagRecordingEvent event = {
        .kind = 1, // Cancelled
        .path = [self->_path UTF8String],
        .path_len =
            [self->_path lengthOfBytesUsingEncoding:NSUTF8StringEncoding],
        .duration_ns = 0,
        .size_bytes = 0,
        .had_audio = self->_hasAudio,
        .error = NULL,
        .error_len = 0};
    self->_callback(&event, self->_context);
  });
}

@end

// ==== Messenger review video ================================================

@interface NametagReviewVideoPlayer : NSObject
@property(nonatomic, strong) AVPlayer *player;
@property(nonatomic, strong) AVPlayerItem *item;
@property(nonatomic, strong) AVPlayerItemVideoOutput *videoOutput;
@property(nonatomic) CVMetalTextureCacheRef textureCache;
@property(nonatomic) CVMetalTextureRef latestYRef;
@property(nonatomic) CVMetalTextureRef latestUVRef;
@property(nonatomic, strong) id<MTLTexture> latestYTexture;
@property(nonatomic, strong) id<MTLTexture> latestUVTexture;
@property(nonatomic, assign) int latestWidth;
@property(nonatomic, assign) int latestHeight;
@property(nonatomic, assign) int latestBitDepth;
@property(nonatomic, assign) int latestMatrix;
@property(nonatomic, assign) int latestVideoRange;
@property(nonatomic, assign) int latestColorSpace;
@property(nonatomic, assign) uint64_t latestTimestampNs;
@property(nonatomic, strong) NSURL *url;
- (instancetype)initWithURL:(NSURL *)url;
- (BOOL)start;
- (void)stop;
- (BOOL)copyLatestTexturesToY:(id<MTLTexture> __strong *)yTexture
                           uv:(id<MTLTexture> __strong *)uvTexture
                        width:(int *)width
                       height:(int *)height
                     bitDepth:(int *)bitDepth
                       matrix:(int *)matrix
                   videoRange:(int *)videoRange
                   colorSpace:(int *)colorSpace
                  timestampNs:(uint64_t *)timestamp;
- (uint16_t)latestRotationDegrees;
@end

@interface NametagReviewVideoPlayer () {
  os_unfair_lock _textureLock;
}
@end

@implementation NametagReviewVideoPlayer

- (instancetype)initWithURL:(NSURL *)url {
  self = [super init];
  if (!self) {
    return nil;
  }
  _textureLock = OS_UNFAIR_LOCK_INIT;
  _textureCache = NULL;
  _latestYRef = NULL;
  _latestUVRef = NULL;
  _latestYTexture = nil;
  _latestUVTexture = nil;
  _latestWidth = 0;
  _latestHeight = 0;
  _latestBitDepth = 8;
  _latestMatrix = 0;
  _latestVideoRange = 0;
  _latestColorSpace = 0;
  _latestTimestampNs = 0;
  _url = url;
  return self;
}

- (void)dealloc {
  [self stop];
  if (_textureCache) {
    CFRelease(_textureCache);
    _textureCache = NULL;
  }
}

- (BOOL)prepareTextureCache {
  return nametag_prepare_texture_cache(&_textureCache);
}

- (void)clearLatestTextures {
  os_unfair_lock_lock(&_textureLock);
  if (self.latestYRef) {
    CFRelease(self.latestYRef);
    self.latestYRef = NULL;
  }
  if (self.latestUVRef) {
    CFRelease(self.latestUVRef);
    self.latestUVRef = NULL;
  }
  self.latestYTexture = nil;
  self.latestUVTexture = nil;
  self.latestWidth = 0;
  self.latestHeight = 0;
  self.latestBitDepth = 8;
  self.latestMatrix = 0;
  self.latestVideoRange = 0;
  self.latestColorSpace = 0;
  self.latestTimestampNs = 0;
  os_unfair_lock_unlock(&_textureLock);
}

- (void)updateLatestTexturesWithY:(CVMetalTextureRef)yRef
                               uv:(CVMetalTextureRef)uvRef
                              bit:(int)bitDepth
                           matrix:(int)matrix
                       videoRange:(int)videoRange
                        timestamp:(uint64_t)timestampNs
                            width:(int)width
                           height:(int)height {
  os_unfair_lock_lock(&_textureLock);
  if (self.latestYRef) {
    CFRelease(self.latestYRef);
    self.latestYRef = NULL;
  }
  if (self.latestUVRef) {
    CFRelease(self.latestUVRef);
    self.latestUVRef = NULL;
  }
  self.latestYRef = yRef ? (CVMetalTextureRef)CFRetain(yRef) : NULL;
  self.latestUVRef = uvRef ? (CVMetalTextureRef)CFRetain(uvRef) : NULL;
  self.latestYTexture =
      self.latestYRef ? CVMetalTextureGetTexture(self.latestYRef) : nil;
  self.latestUVTexture =
      self.latestUVRef ? CVMetalTextureGetTexture(self.latestUVRef) : nil;
  self.latestBitDepth = bitDepth;
  self.latestMatrix = matrix;
  self.latestVideoRange = videoRange;
  self.latestColorSpace = 0;
  self.latestTimestampNs = timestampNs;
  self.latestWidth = width;
  self.latestHeight = height;
  os_unfair_lock_unlock(&_textureLock);
}

- (void)loopVideoFromStart {
  if (self.player == nil) {
    return;
  }
  [self.player seekToTime:kCMTimeZero
          toleranceBefore:kCMTimeZero
           toleranceAfter:kCMTimeZero
        completionHandler:^(BOOL finished) {
          if (finished) {
            [self.player play];
          }
        }];
}

- (void)didReachEnd:(NSNotification *)notification {
  if (notification.object != self.item) {
    return;
  }
  [self loopVideoFromStart];
}

- (BOOL)refreshLatestTextures {
  if (self.player == nil || self.videoOutput == nil ||
      ![self prepareTextureCache]) {
    return NO;
  }
  CMTime itemTime = [self.videoOutput itemTimeForHostTime:CACurrentMediaTime()];
  if (!CMTIME_IS_VALID(itemTime)) {
    itemTime = self.player.currentTime;
  }

  CMTime displayTime = kCMTimeInvalid;
  CVPixelBufferRef pixelBuffer =
      [self.videoOutput copyPixelBufferForItemTime:itemTime
                                itemTimeForDisplay:&displayTime];
  if (pixelBuffer == NULL) {
    return self.latestYTexture != nil && self.latestUVTexture != nil;
  }

  CVMetalTextureRef yRef = NULL;
  CVMetalTextureRef uvRef = NULL;
  int bitDepth = 8;
  int matrix = 0;
  int videoRange = 0;
  int width = 0;
  int height = 0;
  BOOL copied = nametag_copy_nv12_textures_from_pixel_buffer(
      pixelBuffer, self.textureCache, &yRef, &uvRef, &bitDepth, &matrix,
      &videoRange, &width, &height);
  uint64_t timestampNs = timestamp_ns_from_time(
      CMTIME_IS_VALID(displayTime) ? displayTime : itemTime);
  CVPixelBufferRelease(pixelBuffer);
  if (!copied) {
    return self.latestYTexture != nil && self.latestUVTexture != nil;
  }

  [self updateLatestTexturesWithY:yRef
                               uv:uvRef
                              bit:bitDepth
                           matrix:matrix
                       videoRange:videoRange
                        timestamp:timestampNs
                            width:width
                           height:height];
  CFRelease(yRef);
  CFRelease(uvRef);
  return YES;
}

- (BOOL)start {
  if (self.url == nil ||
      ![[NSFileManager defaultManager] fileExistsAtPath:self.url.path]) {
    NSLog(@"[ERROR] review video file missing at path %@", self.url.path);
    return NO;
  }
  if (![self prepareTextureCache]) {
    NSLog(@"[ERROR] review video texture cache unavailable");
    return NO;
  }

  NSDictionary *pixelBufferAttributes = @{
    (id)kCVPixelBufferPixelFormatTypeKey :
        @(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange),
    (id)kCVPixelBufferMetalCompatibilityKey : @YES,
  };
  self.videoOutput = [[AVPlayerItemVideoOutput alloc]
      initWithPixelBufferAttributes:pixelBufferAttributes];
  self.item = [AVPlayerItem playerItemWithURL:self.url];
  [self.item addOutput:self.videoOutput];
  self.player = [AVPlayer playerWithPlayerItem:self.item];
  self.player.actionAtItemEnd = AVPlayerActionAtItemEndNone;
  self.player.volume = 1.0f;
  self.player.muted = NO;
  [[NSNotificationCenter defaultCenter]
      addObserver:self
         selector:@selector(didReachEnd:)
             name:AVPlayerItemDidPlayToEndTimeNotification
           object:self.item];
  [self.player play];
  return YES;
}

- (void)stop {
  if (self.player) {
    [self.player pause];
  }
  if (self.item) {
    [[NSNotificationCenter defaultCenter]
        removeObserver:self
                  name:AVPlayerItemDidPlayToEndTimeNotification
                object:self.item];
  } else {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
  }
  self.player = nil;
  self.item = nil;
  self.videoOutput = nil;
  [self clearLatestTextures];
}

- (BOOL)copyLatestTexturesToY:(id<MTLTexture> __strong *)yTexture
                           uv:(id<MTLTexture> __strong *)uvTexture
                        width:(int *)width
                       height:(int *)height
                     bitDepth:(int *)bitDepth
                       matrix:(int *)matrix
                   videoRange:(int *)videoRange
                   colorSpace:(int *)colorSpace
                  timestampNs:(uint64_t *)timestamp {
  [self refreshLatestTextures];

  os_unfair_lock_lock(&_textureLock);
  id<MTLTexture> y = self.latestYTexture;
  id<MTLTexture> uv = self.latestUVTexture;
  if (!y || !uv) {
    os_unfair_lock_unlock(&_textureLock);
    return NO;
  }
  if (yTexture) {
    *yTexture = y;
  }
  if (uvTexture) {
    *uvTexture = uv;
  }
  if (width) {
    *width = self.latestWidth;
  }
  if (height) {
    *height = self.latestHeight;
  }
  if (bitDepth) {
    *bitDepth = self.latestBitDepth;
  }
  if (matrix) {
    *matrix = self.latestMatrix;
  }
  if (videoRange) {
    *videoRange = self.latestVideoRange;
  }
  if (colorSpace) {
    *colorSpace = self.latestColorSpace;
  }
  if (timestamp) {
    *timestamp = self.latestTimestampNs;
  }
  os_unfair_lock_unlock(&_textureLock);
  return YES;
}

- (uint16_t)latestRotationDegrees {
  return 0;
}

@end

// ==== Capture stream
// ==========================================================

@interface NametagCameraStream
    : NSObject <AVCaptureVideoDataOutputSampleBufferDelegate,
                AVCaptureAudioDataOutputSampleBufferDelegate,
                AVCapturePhotoCaptureDelegate>
@property(nonatomic, strong) AVCaptureSession *session;
@property(nonatomic, strong) AVCaptureDeviceInput *videoInput;
@property(nonatomic, strong) AVCaptureVideoDataOutput *videoOutput;
@property(nonatomic, strong) AVCapturePhotoOutput *photoOutput;
@property(nonatomic, strong) AVCaptureDeviceInput *audioInput;
@property(nonatomic, strong) AVCaptureAudioDataOutput *audioOutput;
@property(nonatomic) dispatch_queue_t sessionQueue;
@property(nonatomic) dispatch_queue_t videoQueue;
@property(nonatomic) dispatch_queue_t audioQueue;
@property(nonatomic, assign) NametagFrameCallback frameCallback;
@property(nonatomic, assign) NametagAudioCallback audioCallback;
@property(nonatomic, assign) void *context;
@property(nonatomic, assign) BOOL enableAudio;
@property(nonatomic, assign) uint32_t desiredWidth;
@property(nonatomic, assign) uint32_t desiredHeight;
@property(nonatomic, assign) uint32_t desiredFps;
@property(nonatomic, assign) AVCaptureDevicePosition desiredPosition;
@property(nonatomic, assign) NSInteger captureMode;
@property(nonatomic, assign) NSInteger previewPixelFormat;
@property(nonatomic, assign, getter=isRunning) BOOL running;
@property(nonatomic) CVMetalTextureCacheRef textureCache;
@property(nonatomic) CVPixelBufferRef latestPixelBuffer;
@property(nonatomic) CVMetalTextureRef latestYRef;
@property(nonatomic) CVMetalTextureRef latestUVRef;
@property(nonatomic) CVMetalTextureRef latestBGRARef;
@property(nonatomic, strong) id<MTLTexture> latestYTexture;
@property(nonatomic, strong) id<MTLTexture> latestUVTexture;
@property(nonatomic, strong) id<MTLTexture> latestBGRATexture;
@property(nonatomic, assign) int latestWidth;
@property(nonatomic, assign) int latestHeight;
@property(nonatomic, assign) int latestBitDepth;
@property(nonatomic, assign) int latestMatrix;
@property(nonatomic, assign) int latestVideoRange;
@property(nonatomic, assign) int latestColorSpace;
@property(nonatomic, assign) uint64_t latestTimestampNs;
@property(nonatomic, assign) uint64_t latestGeneration;
@property(nonatomic, assign) uint16_t latestRotationDeg;
@property(nonatomic, strong)
    NSMutableArray<NametagCameraPublishedSlot *> *publishedSlots;
@property(nonatomic, assign) uint32_t publishCursor;
@property(nonatomic, assign) AVCaptureFlashMode flashMode;
@property(nonatomic, assign) BOOL photoCaptureInFlight;
@property(nonatomic, assign) NametagPhotoCallback photoCallback;
@property(nonatomic, assign) void *photoContext;
@property(nonatomic, assign) BOOL recordingAudioTemporarilyEnabled;
@property(nonatomic, assign) BOOL recordingTorchEnabled;
@property(nonatomic, assign) float lastCaptureTotalMs;
@property(nonatomic, assign) float lastCaptureSampleSetupMs;
@property(nonatomic, assign) float lastCaptureLockMs;
@property(nonatomic, assign) float lastCaptureTextureBridgeMs;
@property(nonatomic, assign) float lastCapturePublishMs;
@property(nonatomic, assign) float lastCapturePublishLockMs;
@property(nonatomic, assign) float lastCapturePublishTextureRefsMs;
@property(nonatomic, assign) float lastCapturePublishPixelBufferMs;
@property(nonatomic, assign) float lastCaptureFrameDeliveryMs;
@property(nonatomic, strong)
    NSMutableDictionary<NSValue *, NSNumber *> *observedSampleSurfaceBytes;
@property(nonatomic, strong) NSMutableDictionary<NSValue *, NSNumber *>
    *observedSampleSurfaceLastSeenFrame;
@property(nonatomic, strong)
    NSMutableSet<NSValue *> *observedReusedSampleSurfaceKeys;
@property(nonatomic, assign) uint64_t observedSampleSurfacePoolBytes;
@property(nonatomic, assign) uint32_t observedSampleSurfaceFrameCount;
@property(nonatomic, assign) uint32_t observedSampleSurfaceReusedFrames;
@property(nonatomic, assign) uint32_t observedSampleSurfaceMaxReuseGapFrames;
@property(nonatomic, strong)
    NSMutableDictionary<NSValue *, NSNumber *> *activeSampleSurfaceRefCounts;
@property(nonatomic, assign) uint64_t activeSampleSurfaceBytes;
@property(nonatomic, assign) uint32_t activeSampleBufferCount;
@property(nonatomic, assign) uint64_t peakActiveSampleSurfaceBytes;
@property(nonatomic, assign) uint32_t peakActiveSampleSurfaceSurfaces;
@property(nonatomic, assign) uint32_t peakActiveSampleBufferCount;
@property(nonatomic, assign) BOOL legacyLatestTextureMirrorCleared;
@property(nonatomic, assign) uint64_t latestPresentedGeneration;
@property(nonatomic, assign) uint32_t observedGenerationAdvances;
@property(nonatomic, assign) uint32_t observedSamplesReceived;
@property(nonatomic, assign) uint32_t observedSamplesDroppedPrebridge;
@property(nonatomic, assign) uint32_t observedSamplesBridged;
@property(nonatomic, assign) uint32_t observedSamplesPublished;
@property(nonatomic, assign) uint32_t observedSamplesPresented;
@property(nonatomic, assign) uint32_t observedSamplesSupersededBeforePresent;
@property(atomic, strong) NametagCameraRecorder *recorder;

- (BOOL)applyConfiguration; // New method for dynamic configuration
- (BOOL)applyConfigurationOnSessionQueue;
- (BOOL)requiresPhotoOutput;
- (BOOL)retainsLatestPreviewPixelBuffer;
- (BOOL)requiresCpuFrameDelivery;
- (BOOL)requiresLegacyLatestTextureMirror;
- (void)clearLegacyLatestTextureMirror;
- (BOOL)capturePhotoWithOptions:(const struct NametagPhotoOptions *)options
                       callback:(NametagPhotoCallback)callback
                        context:(void *)context;
- (BOOL)ensureRecordingAudioCapture;
- (void)disableRecordingAudioCaptureIfNeeded;
- (BOOL)applyTorchModeEnabled:(BOOL)enabled level:(CGFloat)level;
- (void)publishPreviewOnlyTexturesWithY:(CVMetalTextureRef)yRef
                                     uv:(CVMetalTextureRef)uvRef
                                    bit:(int)bitDepth
                                 matrix:(int)matrix
                             videoRange:(int)videoRange
                              timestamp:(uint64_t)timestamp
                          sampleSurface:(NSValue *)sampleSurfaceKey
                     sampleSurfaceBytes:(uint64_t)sampleSurfaceBytes
                                  width:(int)width
                                 height:(int)height;
- (BOOL)copyLatestTextureToBGRA:(id<MTLTexture> __strong *)bgraTexture
                          width:(int *)width
                         height:(int *)height
                    timestampNs:(uint64_t *)timestamp;
- (BOOL)acquireLatestFrame:(struct OxideCamAcquiredFrame *)outFrame
               ifNewerThan:(uint64_t)minGenerationExclusive;
- (void)releaseAcquiredSlot:(uint32_t)slot generation:(uint64_t)generation;
- (uint16_t)latestRotationDegrees;
- (void)copyPerfSnapshot:(struct OxideCamPerfSnapshot *)outSnapshot;
- (void)copyContractSnapshot:(struct OxideCamContractSnapshot *)outSnapshot;
- (void)resetPerfCounters;
- (void)trackActiveSampleBuffer:(CMSampleBufferRef)sampleBuffer
                  sampleSurface:(NSValue *)sampleSurfaceKey
             sampleSurfaceBytes:(uint64_t)sampleSurfaceBytes;
- (void)releaseTrackedActiveSampleBufferForSurface:(NSValue *)sampleSurfaceKey
                                             bytes:(uint64_t)sampleSurfaceBytes;
- (uint64_t)latestPublishedGeneration;
- (uint64_t)latestPublishedTimestampNs;
- (BOOL)shouldDropPreviewOnlySampleBeforeBridge;
- (void)notePublishedFrameGeneration:(uint64_t)generation
                           timestamp:(uint64_t)timestamp;
- (void)notePresentedGeneration:(uint64_t)generation;
@end

@implementation NametagCameraSampleLifetimeToken
- (void)dealloc {
  [self.stream
      releaseTrackedActiveSampleBufferForSurface:self.sampleSurfaceKey
                                           bytes:self.sampleSurfaceBytes];
}
@end

@interface NametagCameraStream () {
  os_unfair_lock _textureLock;
  _Atomic(uint64_t) _publishedFrameState;
  _Atomic(uint32_t) _slotPins[kOxideCameraPublishedSlotCount];
}
@end

@implementation NametagCameraStream

- (instancetype)initWithConfig:(const struct NametagHostCameraConfig *)config
                    frameBlock:(NametagFrameCallback)frameBlock
                    audioBlock:(NametagAudioCallback)audioBlock
                       context:(void *)context {
  self = [super init];
  if (!self) {
    return nil;
  }
  _frameCallback = frameBlock;
  _audioCallback = audioBlock;
  _context = context;
  self.sessionQueue = dispatch_queue_create("com.nametag.camera.session",
                                            DISPATCH_QUEUE_SERIAL);
  dispatch_queue_set_specific(self.sessionQueue, kNametagCameraSessionQueueKey,
                              kNametagCameraSessionQueueKey, NULL);
  self.enableAudio = audioBlock != NULL && config->enable_audio;
  self.desiredWidth = config->width > 0 ? config->width : 1280;
  self.desiredHeight = config->height > 0 ? config->height : 720;
  self.desiredFps = config->fps > 0 ? config->fps : 30;
  self.desiredPosition = (config->device == 0) ? AVCaptureDevicePositionFront
                                               : AVCaptureDevicePositionBack;
  self.captureMode = config->capture_mode;
  self.previewPixelFormat = config->preview_pixel_format == 1 ? 1 : 0;
  _running = NO;
  _textureLock = OS_UNFAIR_LOCK_INIT;
  _textureCache = NULL;
  _latestPixelBuffer = NULL;
  _latestYRef = NULL;
  _latestUVRef = NULL;
  _latestBGRARef = NULL;
  _latestYTexture = nil;
  _latestUVTexture = nil;
  _latestBGRATexture = nil;
  _latestWidth = 0;
  _latestHeight = 0;
  _latestBitDepth = 8;
  _latestMatrix = 0;
  _latestVideoRange = 0;
  _latestColorSpace = 0;
  _latestTimestampNs = 0;
  _latestGeneration = 0;
  _latestRotationDeg = 0;
  _publishedSlots =
      [NSMutableArray arrayWithCapacity:kOxideCameraPublishedSlotCount];
  for (uint32_t idx = 0; idx < kOxideCameraPublishedSlotCount; idx++) {
    [_publishedSlots addObject:[NametagCameraPublishedSlot new]];
    atomic_init(&_slotPins[idx], 0);
  }
  _publishCursor = 0;
  atomic_init(&_publishedFrameState, 0);
  _flashMode = AVCaptureFlashModeOff;
  _photoCaptureInFlight = NO;
  _photoCallback = NULL;
  _photoContext = NULL;
  _recordingAudioTemporarilyEnabled = NO;
  _recordingTorchEnabled = NO;
  _lastCaptureTotalMs = 0.0f;
  _lastCaptureSampleSetupMs = 0.0f;
  _lastCaptureLockMs = 0.0f;
  _lastCaptureTextureBridgeMs = 0.0f;
  _lastCapturePublishMs = 0.0f;
  _lastCapturePublishLockMs = 0.0f;
  _lastCapturePublishTextureRefsMs = 0.0f;
  _lastCapturePublishPixelBufferMs = 0.0f;
  _lastCaptureFrameDeliveryMs = 0.0f;
  _observedSampleSurfaceBytes = [[NSMutableDictionary alloc] init];
  _observedSampleSurfaceLastSeenFrame = [[NSMutableDictionary alloc] init];
  _observedReusedSampleSurfaceKeys = [[NSMutableSet alloc] init];
  _observedSampleSurfacePoolBytes = 0;
  _observedSampleSurfaceFrameCount = 0;
  _observedSampleSurfaceReusedFrames = 0;
  _observedSampleSurfaceMaxReuseGapFrames = 0;
  _latestPresentedGeneration = 0;
  _observedGenerationAdvances = 0;
  _observedSamplesReceived = 0;
  _observedSamplesDroppedPrebridge = 0;
  _observedSamplesBridged = 0;
  _observedSamplesPublished = 0;
  _observedSamplesPresented = 0;
  _observedSamplesSupersededBeforePresent = 0;
  _activeSampleSurfaceRefCounts = [[NSMutableDictionary alloc] init];
  _activeSampleSurfaceBytes = 0;
  _activeSampleBufferCount = 0;
  _peakActiveSampleSurfaceBytes = 0;
  _peakActiveSampleSurfaceSurfaces = 0;
  _peakActiveSampleBufferCount = 0;
  _legacyLatestTextureMirrorCleared = NO;
  return self;
}

- (void)dealloc {
  atomic_store_explicit(&_publishedFrameState, 0, memory_order_release);
  if (_latestPixelBuffer) {
    CVBufferRelease(_latestPixelBuffer);
    _latestPixelBuffer = NULL;
  }
  if (_latestYRef) {
    CFRelease(_latestYRef);
    _latestYRef = NULL;
  }
  if (_latestUVRef) {
    CFRelease(_latestUVRef);
    _latestUVRef = NULL;
  }
  if (_latestBGRARef) {
    CFRelease(_latestBGRARef);
    _latestBGRARef = NULL;
  }
  if (_textureCache) {
    CFRelease(_textureCache);
    _textureCache = NULL;
  }
}

- (int)reservePublishSlot {
  uint32_t slotCount = kOxideCameraPublishedSlotCount;
  if (![self requiresLegacyLatestTextureMirror] &&
      ![self retainsLatestPreviewPixelBuffer]) {
    slotCount = NametagPerfPreviewPublishedSlotCountCurrent();
  }
  if (slotCount == 0 || slotCount > kOxideCameraPublishedSlotCount) {
    slotCount = kOxideCameraPublishedSlotCount;
  }
  uint32_t start = self.publishCursor;
  for (uint32_t offset = 0; offset < slotCount; offset++) {
    uint32_t slot = (start + offset) % slotCount;
    if (atomic_load_explicit(&_slotPins[slot], memory_order_acquire) == 0) {
      self.publishCursor = (slot + 1) % slotCount;
      return (int)slot;
    }
  }
  return -1;
}

- (AVCaptureDevice *)selectDevice {
  AVCaptureDeviceDiscoverySession *discovery = [AVCaptureDeviceDiscoverySession
      discoverySessionWithDeviceTypes:@[
        AVCaptureDeviceTypeBuiltInWideAngleCamera
      ]
                            mediaType:AVMediaTypeVideo
                             position:self.desiredPosition];
  for (AVCaptureDevice *device in discovery.devices) {
    if (device.position == self.desiredPosition) {
      return device;
    }
  }
  return [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
}

- (int)formatRankForSubtype:(FourCharCode)subtype {
  switch (subtype) {
  case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
    return 0;
  case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
    return 1;
  default:
    return INT_MAX;
  }
}

- (BOOL)formatSupportsDesiredFps:(AVCaptureDeviceFormat *)format {
  if (!format || self.desiredFps == 0) {
    return NO;
  }
  CMTime desired = CMTimeMake(1, (int32_t)self.desiredFps);
  for (AVFrameRateRange *range in format.videoSupportedFrameRateRanges) {
    if (CMTimeCompare(desired, range.minFrameDuration) >= 0 &&
        CMTimeCompare(desired, range.maxFrameDuration) <= 0) {
      return YES;
    }
  }
  return NO;
}

- (int)colorSpaceCode:(AVCaptureColorSpace)colorSpace {
  switch (colorSpace) {
  case AVCaptureColorSpace_P3_D65:
    return 1;
  default:
    return 0;
  }
}

- (AVCaptureDeviceFormat *)preferredFormatForDevice:(AVCaptureDevice *)device {
  if (!device) {
    return nil;
  }
  AVCaptureDeviceFormat *best = nil;
  int targetWidth = (int)self.desiredWidth;
  int targetHeight = (int)self.desiredHeight;
  int bestHeightDiff = INT_MAX;
  int bestWidthDiff = INT_MAX;
  int bestRangeRank = INT_MAX;
  for (AVCaptureDeviceFormat *format in device.formats) {
    CMFormatDescriptionRef desc = format.formatDescription;
    if (!desc) {
      continue;
    }
    FourCharCode subtype = CMFormatDescriptionGetMediaSubType(desc);
    int rangeRank = [self formatRankForSubtype:subtype];
    if (rangeRank == INT_MAX || ![self formatSupportsDesiredFps:format]) {
      continue;
    }
    CMVideoDimensions dims = CMVideoFormatDescriptionGetDimensions(desc);
    int heightDiff = abs((int)dims.height - targetHeight);
    int widthDiff = abs((int)dims.width - targetWidth);
    BOOL isBetter =
        heightDiff < bestHeightDiff ||
        (heightDiff == bestHeightDiff && rangeRank < bestRangeRank) ||
        (heightDiff == bestHeightDiff && rangeRank == bestRangeRank &&
         widthDiff < bestWidthDiff);
    if (isBetter) {
      best = format;
      bestHeightDiff = heightDiff;
      bestWidthDiff = widthDiff;
      bestRangeRank = rangeRank;
    }
  }
  return best ?: device.activeFormat;
}

- (void)configureFrameRate:(AVCaptureDevice *)device
                    format:(AVCaptureDeviceFormat *)format {
  if (!device || !format) {
    return;
  }
  CMTime desired = CMTimeMake(1, (int32_t)self.desiredFps);
  NSError *error = nil;
  if (![device lockForConfiguration:&error]) {
    return;
  }
  for (AVFrameRateRange *range in format.videoSupportedFrameRateRanges) {
    if (CMTimeCompare(desired, range.minFrameDuration) >= 0 &&
        CMTimeCompare(desired, range.maxFrameDuration) <= 0) {
      device.activeVideoMinFrameDuration = desired;
      device.activeVideoMaxFrameDuration = desired;
      break;
    }
  }
  [device unlockForConfiguration];
}

- (BOOL)prepareTextureCache {
  return nametag_prepare_texture_cache(&_textureCache);
}

- (BOOL)requiresPhotoOutput {
  return self.captureMode != 0;
}

- (BOOL)retainsLatestPreviewPixelBuffer {
  return [self requiresPhotoOutput];
}

- (BOOL)requiresCpuFrameDelivery {
  return self.frameCallback != NULL && g_oxide_camera_frame_callback != NULL;
}

- (BOOL)requiresLegacyLatestTextureMirror {
  if (self.previewPixelFormat == 1) {
    return YES;
  }
  return [self requiresPhotoOutput] || [self requiresCpuFrameDelivery];
}

- (void)clearLegacyLatestTextureMirror {
  if (self.legacyLatestTextureMirrorCleared) {
    return;
  }
  os_unfair_lock_lock(&_textureLock);
  if (self.latestPixelBuffer) {
    CVBufferRelease(self.latestPixelBuffer);
    self.latestPixelBuffer = NULL;
  }
  if (self.latestYRef) {
    CFRelease(self.latestYRef);
    self.latestYRef = NULL;
  }
  if (self.latestUVRef) {
    CFRelease(self.latestUVRef);
    self.latestUVRef = NULL;
  }
  if (self.latestBGRARef) {
    CFRelease(self.latestBGRARef);
    self.latestBGRARef = NULL;
  }
  self.latestYTexture = nil;
  self.latestUVTexture = nil;
  self.latestBGRATexture = nil;
  os_unfair_lock_unlock(&_textureLock);
  self.legacyLatestTextureMirrorCleared = YES;
}

- (BOOL)shouldDropPreviewOnlySampleBeforeBridge {
  if (!NametagPerfPreviewPrebridgeDropEnabled() ||
      [self requiresLegacyLatestTextureMirror] ||
      [self retainsLatestPreviewPixelBuffer] ||
      [self requiresCpuFrameDelivery]) {
    return NO;
  }
  os_unfair_lock_lock(&_textureLock);
  uint64_t latestPublishedGeneration = self.latestGeneration;
  uint64_t latestPresentedGeneration = self.latestPresentedGeneration;
  os_unfair_lock_unlock(&_textureLock);
  return latestPublishedGeneration > latestPresentedGeneration;
}

- (void)notePublishedFrameGeneration:(uint64_t)generation
                           timestamp:(uint64_t)timestamp {
  BOOL supersededBeforePresent = NO;
  os_unfair_lock_lock(&_textureLock);
  supersededBeforePresent =
      self.latestGeneration > self.latestPresentedGeneration;
  self.latestGeneration = generation;
  self.observedGenerationAdvances = self.observedGenerationAdvances + 1;
  self.observedSamplesPublished = self.observedSamplesPublished + 1;
  if (supersededBeforePresent) {
    self.observedSamplesSupersededBeforePresent =
        self.observedSamplesSupersededBeforePresent + 1;
  }
  os_unfair_lock_unlock(&_textureLock);
  OxideDispatchPreviewPublishCallback(generation, timestamp);
}

- (void)notePresentedGeneration:(uint64_t)generation {
  if (generation == 0) {
    return;
  }
  os_unfair_lock_lock(&_textureLock);
  if (generation > self.latestPresentedGeneration) {
    self.latestPresentedGeneration = generation;
    self.observedSamplesPresented = self.observedSamplesPresented + 1;
  }
  os_unfair_lock_unlock(&_textureLock);
}

- (BOOL)applyConfiguration {
  __block BOOL result = NO;
  NametagDispatchBlockingQueue(
      self.sessionQueue, kNametagCameraSessionQueueKey, ^{
        result = [self applyConfigurationOnSessionQueue];
      });
  return result;
}

- (BOOL)applyConfigurationOnSessionQueue {
  if (!self.session) {
    NSLog(@"[ERROR] AVCaptureSession is not initialized.");
    return NO;
  }

  self.session.automaticallyConfiguresApplicationAudioSession = NO;
  if (@available(iOS 10.0, *)) {
    self.session.automaticallyConfiguresCaptureDeviceForWideColor = NO;
  }

  [self.session beginConfiguration];

  // Remove existing inputs/outputs if they are about to change
  if (self.videoInput) {
    [self.session removeInput:self.videoInput];
    self.videoInput = nil;
  }
  if (self.audioInput) {
    [self.session removeInput:self.audioInput];
    self.audioInput = nil;
  }
  if (self.videoOutput) {
    [self.session removeOutput:self.videoOutput];
    self.videoOutput = nil;
  }
  if (self.photoOutput) {
    [self.session removeOutput:self.photoOutput];
    self.photoOutput = nil;
  }
  if (self.audioOutput) {
    [self.session removeOutput:self.audioOutput];
    self.audioOutput = nil;
  }
  if (![self retainsLatestPreviewPixelBuffer]) {
    os_unfair_lock_lock(&_textureLock);
    if (self.latestPixelBuffer) {
      CVBufferRelease(self.latestPixelBuffer);
      self.latestPixelBuffer = NULL;
    }
    os_unfair_lock_unlock(&_textureLock);
  }
  if ([self requiresLegacyLatestTextureMirror]) {
    self.legacyLatestTextureMirrorCleared = NO;
  } else {
    [self clearLegacyLatestTextureMirror];
  }
  if (NametagPerfParkedModeCurrent()) {
    [self resetPerfCounters];
  }

  // Configure Video Input
  AVCaptureDevice *videoDevice = [self selectDevice];
  if (!videoDevice) {
    NSLog(@"[ERROR] Failed to select video device.");
    [self.session commitConfiguration];
    return NO;
  }

  NSError *error = nil;
  AVCaptureDeviceInput *newVideoInput =
      [AVCaptureDeviceInput deviceInputWithDevice:videoDevice error:&error];
  if (!newVideoInput || error) {
    NSLog(@"[ERROR] Failed to create video input: %@", error);
    [self.session commitConfiguration];
    return NO;
  }

  if ([self.session canAddInput:newVideoInput]) {
    [self.session addInput:newVideoInput];
    self.videoInput = newVideoInput;
  } else {
    NSLog(@"[ERROR] Cannot add video input to session.");
    [self.session commitConfiguration];
    return NO;
  }

  // Configure Video Format and Frame Rate
  AVCaptureDeviceFormat *format = nil;
  NSError *fmtErr = nil;
  if ([videoDevice lockForConfiguration:&fmtErr]) {
    switch (NametagPerfCameraCaptureContractModeCurrent()) {
    case NametagPerfCameraCaptureContractPreset720p:
      if (![self.session canSetSessionPreset:AVCaptureSessionPreset1280x720]) {
        [videoDevice unlockForConfiguration];
        NSLog(@"[ERROR] Cannot set 720p session preset.");
        [self.session commitConfiguration];
        return NO;
      }
      self.session.sessionPreset = AVCaptureSessionPreset1280x720;
      format = videoDevice.activeFormat;
      break;
    case NametagPerfCameraCaptureContractInputPriority:
    default:
      format = [self preferredFormatForDevice:videoDevice];
      if (![self.session
              canSetSessionPreset:AVCaptureSessionPresetInputPriority]) {
        [videoDevice unlockForConfiguration];
        NSLog(@"[ERROR] Cannot set input-priority session preset.");
        [self.session commitConfiguration];
        return NO;
      }
      self.session.sessionPreset = AVCaptureSessionPresetInputPriority;
      if (format != nil) {
        videoDevice.activeFormat = format;
      }
      break;
    }
    if (@available(iOS 10.0, *)) {
      AVCaptureDeviceFormat *activeFormat = videoDevice.activeFormat;
      if ([activeFormat.supportedColorSpaces
              containsObject:@(AVCaptureColorSpace_sRGB)]) {
        videoDevice.activeColorSpace = AVCaptureColorSpace_sRGB;
      }
      self.latestColorSpace =
          [self colorSpaceCode:videoDevice.activeColorSpace];
    } else {
      self.latestColorSpace = 0;
    }
    [videoDevice unlockForConfiguration];
  } else {
    NSLog(@"[ERROR] Failed to lock video device for format configuration: %@",
          fmtErr);
    [self.session commitConfiguration];
    return NO;
  }
  [self configureFrameRate:videoDevice
                    format:format ?: videoDevice.activeFormat];

  // Configure Video Output
  self.videoOutput = [[AVCaptureVideoDataOutput alloc] init];
  self.videoOutput.alwaysDiscardsLateVideoFrames = YES;
  OSType previewPixelFormat =
      self.previewPixelFormat == 1
          ? kCVPixelFormatType_32BGRA
          : kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;
  self.videoOutput.videoSettings =
      @{(id)kCVPixelBufferPixelFormatTypeKey : @(previewPixelFormat)};
  self.videoQueue =
      dispatch_queue_create("com.nametag.camera.video", DISPATCH_QUEUE_SERIAL);
  dispatch_set_target_queue(
      self.videoQueue,
      dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0));
  [self.videoOutput setSampleBufferDelegate:self queue:self.videoQueue];
  if ([self.session canAddOutput:self.videoOutput]) {
    [self.session addOutput:self.videoOutput];
  } else {
    NSLog(@"[ERROR] Cannot add video output to session.");
    [self.session commitConfiguration];
    return NO;
  }

  if ([self requiresPhotoOutput]) {
    self.photoOutput = [[AVCapturePhotoOutput alloc] init];
    if ([self.session canAddOutput:self.photoOutput]) {
      [self.session addOutput:self.photoOutput];
    } else {
      NSLog(@"[ERROR] Cannot add photo output to session.");
      self.photoOutput = nil;
      [self.session commitConfiguration];
      return NO;
    }
  }

  // Configure Audio Input/Output if enabled
  if (self.enableAudio) {
    AVCaptureDevice *audioDevice =
        [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
    if (audioDevice) {
      NSError *audioErr = nil;
      AVCaptureDeviceInput *newAudioInput =
          [AVCaptureDeviceInput deviceInputWithDevice:audioDevice
                                                error:&audioErr];
      if (newAudioInput && !audioErr &&
          [self.session canAddInput:newAudioInput]) {
        [self.session addInput:newAudioInput];
        self.audioInput = newAudioInput;
        self.audioOutput = [[AVCaptureAudioDataOutput alloc] init];
        self.audioQueue = dispatch_queue_create("com.nametag.camera.audio",
                                                DISPATCH_QUEUE_SERIAL);
        [self.audioOutput setSampleBufferDelegate:self queue:self.audioQueue];
        if ([self.session canAddOutput:self.audioOutput]) {
          [self.session addOutput:self.audioOutput];
        } else {
          NSLog(@"[ERROR] Cannot add audio output to session.");
          self.audioOutput = nil;
          self.audioQueue = NULL;
        }
      } else {
        NSLog(@"[ERROR] Failed to create or add audio input: %@", audioErr);
      }
    } else {
      NSLog(@"[ERROR] No audio device found.");
    }
  }

  AVCaptureConnection *connection =
      [self.videoOutput connectionWithMediaType:AVMediaTypeVideo];
  if (connection) {
    const CGFloat portraitAngle = 90.0;
    if ([connection isVideoRotationAngleSupported:portraitAngle]) {
      connection.videoRotationAngle = portraitAngle;
    }
    connection.automaticallyAdjustsVideoMirroring = NO;
    connection.videoMirrored =
        (self.desiredPosition == AVCaptureDevicePositionFront);
  }

  AVCaptureConnection *photoConnection =
      self.photoOutput
          ? [self.photoOutput connectionWithMediaType:AVMediaTypeVideo]
          : nil;
  if (photoConnection) {
    const CGFloat portraitAngle = 90.0;
    if ([photoConnection isVideoRotationAngleSupported:portraitAngle]) {
      photoConnection.videoRotationAngle = portraitAngle;
    }
    photoConnection.automaticallyAdjustsVideoMirroring = NO;
    photoConnection.videoMirrored =
        (self.desiredPosition == AVCaptureDevicePositionFront);
  }

  [self.session commitConfiguration];
  return YES;
}

- (BOOL)start {
  __block BOOL started = NO;
  NametagDispatchBlockingQueue(
      self.sessionQueue, kNametagCameraSessionQueueKey, ^{
        if (self.running) {
          started = YES;
          return;
        }

        if (![self prepareTextureCache]) {
          return;
        }

        self.session = [[AVCaptureSession alloc] init];
        self.session.automaticallyConfiguresApplicationAudioSession = NO;
        if (![self applyConfigurationOnSessionQueue]) {
          self.session = nil;
          return;
        }

        [self.session startRunning];
        self.running = YES;
        started = YES;
      });
  return started;
}

- (void)stop {
  NametagDispatchBlockingQueue(
      self.sessionQueue, kNametagCameraSessionQueueKey, ^{
        if (!self.running) {
          return;
        }
        [self.session stopRunning];
        if (self.videoOutput) {
          [self.videoOutput setSampleBufferDelegate:nil queue:NULL];
        }
        if (self.audioOutput) {
          [self.audioOutput setSampleBufferDelegate:nil queue:NULL];
        }

        os_unfair_lock_lock(&_textureLock);
        if (self.latestPixelBuffer) {
          CVBufferRelease(self.latestPixelBuffer);
          self.latestPixelBuffer = NULL;
        }
        if (self.latestYRef) {
          CFRelease(self.latestYRef);
          self.latestYRef = NULL;
        }
        if (self.latestUVRef) {
          CFRelease(self.latestUVRef);
          self.latestUVRef = NULL;
        }
        if (self.latestBGRARef) {
          CFRelease(self.latestBGRARef);
          self.latestBGRARef = NULL;
        }
        self.latestYTexture = nil;
        self.latestUVTexture = nil;
        self.latestBGRATexture = nil;
        self.latestTimestampNs = 0;
        self.latestGeneration = 0;
        self.latestPresentedGeneration = 0;
        self.legacyLatestTextureMirrorCleared = NO;
        self.publishCursor = 0;
        atomic_store_explicit(&_publishedFrameState, 0, memory_order_release);
        os_unfair_lock_unlock(&_textureLock);

        self.session = nil;
        self.videoInput = nil;
        self.videoOutput = nil;
        self.photoOutput = nil;
        self.audioInput = nil;
        self.audioOutput = nil;
        self.videoQueue = NULL;
        self.audioQueue = NULL;
        self.photoCaptureInFlight = NO;
        self.photoCallback = NULL;
        self.photoContext = NULL;
        self.recordingAudioTemporarilyEnabled = NO;
        self.recordingTorchEnabled = NO;
        self.running = NO;
      });
}

- (BOOL)ensureRecordingAudioCapture {
  if (self.audioOutput != nil && self.audioInput != nil) {
    return YES;
  }
  if (self.session == nil) {
    return NO;
  }
  AVCaptureDevice *audioDevice =
      [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeAudio];
  if (!audioDevice) {
    NSLog(@"[ERROR] No audio device found for recording.");
    return NO;
  }
  NSError *audioErr = nil;
  AVCaptureDeviceInput *newAudioInput =
      [AVCaptureDeviceInput deviceInputWithDevice:audioDevice error:&audioErr];
  if (!newAudioInput || audioErr) {
    NSLog(@"[ERROR] Failed to create recording audio input: %@", audioErr);
    return NO;
  }
  AVCaptureAudioDataOutput *newAudioOutput =
      [[AVCaptureAudioDataOutput alloc] init];
  dispatch_queue_t newAudioQueue = dispatch_queue_create(
      "com.nametag.camera.audio.recording", DISPATCH_QUEUE_SERIAL);
  [newAudioOutput setSampleBufferDelegate:self queue:newAudioQueue];

  [self.session beginConfiguration];
  BOOL addedInput = NO;
  BOOL addedOutput = NO;
  if ([self.session canAddInput:newAudioInput]) {
    [self.session addInput:newAudioInput];
    addedInput = YES;
  }
  if ([self.session canAddOutput:newAudioOutput]) {
    [self.session addOutput:newAudioOutput];
    addedOutput = YES;
  }
  if (!addedInput || !addedOutput) {
    if (addedInput) {
      [self.session removeInput:newAudioInput];
    }
    if (addedOutput) {
      [self.session removeOutput:newAudioOutput];
    }
    [self.session commitConfiguration];
    NSLog(@"[ERROR] Failed to attach recording audio capture.");
    return NO;
  }
  [self.session commitConfiguration];

  self.audioInput = newAudioInput;
  self.audioOutput = newAudioOutput;
  self.audioQueue = newAudioQueue;
  self.recordingAudioTemporarilyEnabled = !self.enableAudio;
  (void)[[AVAudioSession sharedInstance] setActive:YES error:nil];
  return YES;
}

- (void)disableRecordingAudioCaptureIfNeeded {
  if (!self.recordingAudioTemporarilyEnabled || self.session == nil) {
    return;
  }
  [self.session beginConfiguration];
  if (self.audioInput) {
    [self.session removeInput:self.audioInput];
    self.audioInput = nil;
  }
  if (self.audioOutput) {
    [self.audioOutput setSampleBufferDelegate:nil queue:NULL];
    [self.session removeOutput:self.audioOutput];
    self.audioOutput = nil;
  }
  [self.session commitConfiguration];
  self.audioQueue = NULL;
  self.recordingAudioTemporarilyEnabled = NO;
  (void)[[AVAudioSession sharedInstance] setActive:NO error:nil];
}

- (BOOL)applyTorchModeEnabled:(BOOL)enabled level:(CGFloat)level {
  AVCaptureDevice *device = self.videoInput.device;
  if (!device || !device.hasTorch) {
    self.recordingTorchEnabled = NO;
    return !enabled;
  }

  NSError *error = nil;
  if (![device lockForConfiguration:&error]) {
    NSLog(@"[ERROR] Failed to lock torch configuration: %@", error);
    self.recordingTorchEnabled = NO;
    return NO;
  }

  BOOL success = YES;
  if (enabled) {
    if ([device isTorchModeSupported:AVCaptureTorchModeOn]) {
      CGFloat requestedLevel = level;
      if (!isfinite(requestedLevel) || requestedLevel <= 0.0) {
        requestedLevel = AVCaptureMaxAvailableTorchLevel;
      }
      CGFloat clampedLevel =
          MIN(MAX(requestedLevel, 0.0), AVCaptureMaxAvailableTorchLevel);
      success = [device setTorchModeOnWithLevel:clampedLevel error:&error];
    } else {
      success = NO;
    }
  } else if ([device isTorchModeSupported:AVCaptureTorchModeOff]) {
    device.torchMode = AVCaptureTorchModeOff;
  }
  [device unlockForConfiguration];

  if (!success || error) {
    NSLog(@"[ERROR] Failed to update recording torch: %@", error);
    self.recordingTorchEnabled = NO;
    return NO;
  }
  self.recordingTorchEnabled = enabled;
  return YES;
}

- (void)publishPreviewOnlyTexturesWithY:(CVMetalTextureRef)yRef
                                     uv:(CVMetalTextureRef)uvRef
                                    bit:(int)bitDepth
                                 matrix:(int)matrix
                             videoRange:(int)videoRange
                              timestamp:(uint64_t)timestamp
                          sampleSurface:(NSValue *)sampleSurfaceKey
                     sampleSurfaceBytes:(uint64_t)sampleSurfaceBytes
                                  width:(int)width
                                 height:(int)height {
  NSCAssert(![self requiresLegacyLatestTextureMirror],
            @"preview-only publication must not touch the legacy "
            @"latest-texture mirror");
  NSCAssert(![self retainsLatestPreviewPixelBuffer],
            @"preview-only publication must not retain a latest pixel buffer");
  [self clearLegacyLatestTextureMirror];
  os_unfair_lock_lock(&_textureLock);
  self.latestBitDepth = bitDepth;
  self.latestMatrix = matrix;
  self.latestVideoRange = videoRange;
  self.latestTimestampNs = timestamp;
  self.latestRotationDeg = 0;
  self.latestWidth = width;
  self.latestHeight = height;
  self.lastCapturePublishLockMs = 0.0f;
  self.lastCapturePublishTextureRefsMs = 0.0f;
  self.lastCapturePublishPixelBufferMs = 0.0f;
  os_unfair_lock_unlock(&_textureLock);

  int slotIndex = [self reservePublishSlot];
  if (slotIndex < 0) {
    if (yRef != NULL) {
      CFRelease(yRef);
    }
    if (uvRef != NULL) {
      CFRelease(uvRef);
    }
    return;
  }
  NametagCameraPublishedSlot *slot = self.publishedSlots[(NSUInteger)slotIndex];
  if (slot.yRef != NULL) {
    CFRelease(slot.yRef);
    slot.yRef = NULL;
  }
  if (slot.uvRef != NULL) {
    CFRelease(slot.uvRef);
    slot.uvRef = NULL;
  }
  slot.yRef = yRef;
  slot.uvRef = uvRef;
  slot.yTex = slot.yRef != NULL ? CVMetalTextureGetTexture(slot.yRef) : nil;
  slot.uvTex = slot.uvRef != NULL ? CVMetalTextureGetTexture(slot.uvRef) : nil;
  slot.sampleSurfaceKey = sampleSurfaceKey;
  slot.sampleSurfaceBytes = sampleSurfaceBytes;
  slot.width = width;
  slot.height = height;
  slot.bitDepth = bitDepth;
  slot.matrix = matrix;
  slot.videoRange = videoRange;
  slot.colorSpace = self.latestColorSpace;
  uint64_t nextGeneration = self.latestGeneration + 1;
  slot.generation = nextGeneration;
  slot.timestampNs = timestamp;
  atomic_store_explicit(
      &_publishedFrameState,
      OxidePackPublishedFrameState((uint32_t)slotIndex, slot.generation),
      memory_order_release);
  [self notePublishedFrameGeneration:nextGeneration timestamp:timestamp];
}

- (void)updateLatestTexturesWithY:(CVMetalTextureRef)yRef
                               uv:(CVMetalTextureRef)uvRef
                    sampleSurface:(NSValue *)sampleSurfaceKey
               sampleSurfaceBytes:(uint64_t)sampleSurfaceBytes
                      pixelBuffer:(CVPixelBufferRef)pixelBuffer
                              bit:(int)bitDepth
                           matrix:(int)matrix
                       videoRange:(int)videoRange
                        timestamp:(uint64_t)timestamp
                            width:(int)width
                           height:(int)height {
  self.legacyLatestTextureMirrorCleared = NO;
  uint64_t lockTicks = NametagPerfNowTicks();
  NAMETAG_PERF_BEGIN(lockSignpost, "camera.capture.publish.lock");
  os_unfair_lock_lock(&_textureLock);
  NAMETAG_PERF_END(lockSignpost, "camera.capture.publish.lock");
  float lockMs = (float)NametagPerfElapsedMs(lockTicks);
  uint64_t textureRefTicks = NametagPerfNowTicks();
  NAMETAG_PERF_BEGIN(textureRefSignpost, "camera.capture.publish.texture_refs");
  if (self.latestYRef) {
    CFRelease(self.latestYRef);
    self.latestYRef = NULL;
  }
  if (self.latestUVRef) {
    CFRelease(self.latestUVRef);
    self.latestUVRef = NULL;
  }
  if (self.latestBGRARef) {
    CFRelease(self.latestBGRARef);
    self.latestBGRARef = NULL;
  }
  NAMETAG_PERF_END(textureRefSignpost, "camera.capture.publish.texture_refs");
  float textureRefMs = (float)NametagPerfElapsedMs(textureRefTicks);
  uint64_t pixelBufferTicks = NametagPerfNowTicks();
  NAMETAG_PERF_BEGIN(pixelBufferSignpost,
                     "camera.capture.publish.pixel_buffer");
  if (self.latestPixelBuffer) {
    CVBufferRelease(self.latestPixelBuffer);
    self.latestPixelBuffer = NULL;
  }
  self.latestYRef = yRef != NULL ? (CVMetalTextureRef)CFRetain(yRef) : NULL;
  self.latestUVRef = uvRef != NULL ? (CVMetalTextureRef)CFRetain(uvRef) : NULL;
  if (pixelBuffer && [self retainsLatestPreviewPixelBuffer]) {
    self.latestPixelBuffer = (CVPixelBufferRef)CVBufferRetain(pixelBuffer);
  }
  NAMETAG_PERF_END(pixelBufferSignpost, "camera.capture.publish.pixel_buffer");
  float pixelBufferMs = (float)NametagPerfElapsedMs(pixelBufferTicks);
  self.latestYTexture =
      self.latestYRef ? CVMetalTextureGetTexture(self.latestYRef) : nil;
  self.latestUVTexture =
      self.latestUVRef ? CVMetalTextureGetTexture(self.latestUVRef) : nil;
  self.latestBGRATexture = nil;
  self.latestBitDepth = bitDepth;
  self.latestMatrix = matrix;
  self.latestVideoRange = videoRange;
  self.latestTimestampNs = timestamp;
  self.latestRotationDeg = 0;
  self.latestWidth = width;
  self.latestHeight = height;
  self.lastCapturePublishLockMs = lockMs;
  self.lastCapturePublishTextureRefsMs = textureRefMs;
  self.lastCapturePublishPixelBufferMs = pixelBufferMs;
  os_unfair_lock_unlock(&_textureLock);

  int slotIndex = [self reservePublishSlot];
  if (slotIndex < 0) {
    if (yRef != NULL) {
      CFRelease(yRef);
    }
    if (uvRef != NULL) {
      CFRelease(uvRef);
    }
    return;
  }
  NametagCameraPublishedSlot *slot = self.publishedSlots[(NSUInteger)slotIndex];
  if (slot.yRef != NULL) {
    CFRelease(slot.yRef);
    slot.yRef = NULL;
  }
  if (slot.uvRef != NULL) {
    CFRelease(slot.uvRef);
    slot.uvRef = NULL;
  }
  slot.yRef = yRef;
  slot.uvRef = uvRef;
  slot.yTex = slot.yRef != NULL ? CVMetalTextureGetTexture(slot.yRef) : nil;
  slot.uvTex = slot.uvRef != NULL ? CVMetalTextureGetTexture(slot.uvRef) : nil;
  slot.sampleSurfaceKey = sampleSurfaceKey;
  slot.sampleSurfaceBytes = sampleSurfaceBytes;
  slot.width = width;
  slot.height = height;
  slot.bitDepth = bitDepth;
  slot.matrix = matrix;
  slot.videoRange = videoRange;
  slot.colorSpace = self.latestColorSpace;
  uint64_t nextGeneration = self.latestGeneration + 1;
  slot.generation = nextGeneration;
  slot.timestampNs = timestamp;
  atomic_store_explicit(
      &_publishedFrameState,
      OxidePackPublishedFrameState((uint32_t)slotIndex, slot.generation),
      memory_order_release);
  [self notePublishedFrameGeneration:nextGeneration timestamp:timestamp];
}

- (void)updateLatestTextureWithBGRA:(CVMetalTextureRef)bgraRef
                        pixelBuffer:(CVPixelBufferRef)pixelBuffer
                          timestamp:(uint64_t)timestamp
                              width:(int)width
                             height:(int)height {
  BOOL requiresLegacyMirror = [self requiresLegacyLatestTextureMirror];
  if (!requiresLegacyMirror) {
    [self clearLegacyLatestTextureMirror];
    os_unfair_lock_lock(&_textureLock);
    self.latestBitDepth = 8;
    self.latestMatrix = 0;
    self.latestVideoRange = 0;
    self.latestTimestampNs = timestamp;
    self.latestRotationDeg = 0;
    self.latestWidth = width;
    self.latestHeight = height;
    self.lastCapturePublishLockMs = 0.0f;
    self.lastCapturePublishTextureRefsMs = 0.0f;
    self.lastCapturePublishPixelBufferMs = 0.0f;
    os_unfair_lock_unlock(&_textureLock);
    return;
  }
  self.legacyLatestTextureMirrorCleared = NO;
  uint64_t lockTicks = NametagPerfNowTicks();
  NAMETAG_PERF_BEGIN(lockSignpost, "camera.capture.publish.lock");
  os_unfair_lock_lock(&_textureLock);
  NAMETAG_PERF_END(lockSignpost, "camera.capture.publish.lock");
  float lockMs = (float)NametagPerfElapsedMs(lockTicks);
  uint64_t textureRefTicks = NametagPerfNowTicks();
  NAMETAG_PERF_BEGIN(textureRefSignpost, "camera.capture.publish.texture_refs");
  if (self.latestYRef) {
    CFRelease(self.latestYRef);
    self.latestYRef = NULL;
  }
  if (self.latestUVRef) {
    CFRelease(self.latestUVRef);
    self.latestUVRef = NULL;
  }
  if (self.latestBGRARef) {
    CFRelease(self.latestBGRARef);
    self.latestBGRARef = NULL;
  }
  NAMETAG_PERF_END(textureRefSignpost, "camera.capture.publish.texture_refs");
  float textureRefMs = (float)NametagPerfElapsedMs(textureRefTicks);
  uint64_t pixelBufferTicks = NametagPerfNowTicks();
  NAMETAG_PERF_BEGIN(pixelBufferSignpost,
                     "camera.capture.publish.pixel_buffer");
  if (self.latestPixelBuffer) {
    CVBufferRelease(self.latestPixelBuffer);
    self.latestPixelBuffer = NULL;
  }
  self.latestBGRARef = bgraRef;
  if (pixelBuffer && [self retainsLatestPreviewPixelBuffer]) {
    self.latestPixelBuffer = (CVPixelBufferRef)CVBufferRetain(pixelBuffer);
  }
  NAMETAG_PERF_END(pixelBufferSignpost, "camera.capture.publish.pixel_buffer");
  float pixelBufferMs = (float)NametagPerfElapsedMs(pixelBufferTicks);
  self.latestYTexture = nil;
  self.latestUVTexture = nil;
  self.latestBGRATexture =
      self.latestBGRARef ? CVMetalTextureGetTexture(self.latestBGRARef) : nil;
  self.latestBitDepth = 8;
  self.latestMatrix = 0;
  self.latestVideoRange = 0;
  self.latestTimestampNs = timestamp;
  self.latestRotationDeg = 0;
  self.latestWidth = width;
  self.latestHeight = height;
  self.lastCapturePublishLockMs = lockMs;
  self.lastCapturePublishTextureRefsMs = textureRefMs;
  self.lastCapturePublishPixelBufferMs = pixelBufferMs;
  os_unfair_lock_unlock(&_textureLock);
}

- (BOOL)copyLatestTexturesToY:(id<MTLTexture> __strong *)yTexture
                           uv:(id<MTLTexture> __strong *)uvTexture
                        width:(int *)width
                       height:(int *)height
                     bitDepth:(int *)bitDepth
                       matrix:(int *)matrix
                   videoRange:(int *)videoRange
                   colorSpace:(int *)colorSpace
                  timestampNs:(uint64_t *)timestamp {
  os_unfair_lock_lock(&_textureLock);
  id<MTLTexture> y = self.latestYTexture;
  id<MTLTexture> uv = self.latestUVTexture;
  if (!y || !uv) {
    os_unfair_lock_unlock(&_textureLock);
    return NO;
  }
  if (yTexture) {
    *yTexture = y;
  }
  if (uvTexture) {
    *uvTexture = uv;
  }
  if (width) {
    *width = self.latestWidth;
  }
  if (height) {
    *height = self.latestHeight;
  }
  if (bitDepth) {
    *bitDepth = self.latestBitDepth;
  }
  if (matrix) {
    *matrix = self.latestMatrix;
  }
  if (videoRange) {
    *videoRange = self.latestVideoRange;
  }
  if (colorSpace) {
    *colorSpace = self.latestColorSpace;
  }
  if (timestamp) {
    *timestamp = self.latestTimestampNs;
  }
  os_unfair_lock_unlock(&_textureLock);
  return YES;
}

- (BOOL)acquireLatestFrame:(struct OxideCamAcquiredFrame *)outFrame
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
    uint64_t generation = OxidePublishedFrameGeneration(publishedState);
    if (generation == 0 || generation <= minGenerationExclusive) {
      return NO;
    }
    uint32_t slotIndex = OxidePublishedFrameSlot(publishedState);
    if (slotIndex >= kOxideCameraPublishedSlotCount ||
        slotIndex >= self.publishedSlots.count) {
      return NO;
    }
    atomic_fetch_add_explicit(&_slotPins[slotIndex], 1, memory_order_acq_rel);
    NametagCameraPublishedSlot *slot =
        self.publishedSlots[(NSUInteger)slotIndex];
    if (slot.generation != generation) {
      atomic_fetch_sub_explicit(&_slotPins[slotIndex], 1, memory_order_acq_rel);
      continue;
    }
    if (slot.yTex == nil || slot.uvTex == nil || slot.width <= 0 ||
        slot.height <= 0) {
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

- (void)releaseAcquiredSlot:(uint32_t)slot generation:(uint64_t)generation {
  if (generation == 0 || slot >= kOxideCameraPublishedSlotCount ||
      slot >= self.publishedSlots.count) {
    return;
  }
  NametagCameraPublishedSlot *publishedSlot =
      self.publishedSlots[(NSUInteger)slot];
  if (publishedSlot.generation != generation) {
    return;
  }
  atomic_fetch_sub_explicit(&_slotPins[slot], 1, memory_order_acq_rel);
}

- (BOOL)copyLatestTextureToBGRA:(id<MTLTexture> __strong *)bgraTexture
                          width:(int *)width
                         height:(int *)height
                    timestampNs:(uint64_t *)timestamp {
  os_unfair_lock_lock(&_textureLock);
  id<MTLTexture> bgra = self.latestBGRATexture;
  if (!bgra) {
    os_unfair_lock_unlock(&_textureLock);
    return NO;
  }
  if (bgraTexture) {
    *bgraTexture = bgra;
  }
  if (width) {
    *width = self.latestWidth;
  }
  if (height) {
    *height = self.latestHeight;
  }
  if (timestamp) {
    *timestamp = self.latestTimestampNs;
  }
  os_unfair_lock_unlock(&_textureLock);
  return YES;
}

- (uint16_t)latestRotationDegrees {
  os_unfair_lock_lock(&_textureLock);
  uint16_t rotationDeg = self.latestRotationDeg;
  os_unfair_lock_unlock(&_textureLock);
  return rotationDeg;
}

- (void)copyPerfSnapshot:(struct OxideCamPerfSnapshot *)outSnapshot {
  if (outSnapshot == NULL) {
    return;
  }
  os_unfair_lock_lock(&_textureLock);
  uint64_t retainedSampleSurfaceBytes = 0;
  uint32_t retainedSampleSurfaceSurfaces = 0;
  uint64_t retainedPublishedSlotSurfaceBytes = 0;
  uint32_t retainedPublishedSlotSurfaceSurfaces = 0;
  uint64_t retainedLatestPixelBufferSurfaceBytes = 0;
  uint32_t retainedLatestPixelBufferSurfaceSurfaces = 0;
  NSMutableSet<NSValue *> *retainedSurfaceKeys = [[NSMutableSet alloc] init];
  NSMutableSet<NSValue *> *slotSurfaceKeys = [[NSMutableSet alloc] init];
  for (NametagCameraPublishedSlot *slot in self.publishedSlots) {
    if (slot.sampleSurfaceKey == nil || slot.sampleSurfaceBytes == 0 ||
        [slotSurfaceKeys containsObject:slot.sampleSurfaceKey]) {
      continue;
    }
    [slotSurfaceKeys addObject:slot.sampleSurfaceKey];
    retainedPublishedSlotSurfaceBytes += slot.sampleSurfaceBytes;
    retainedPublishedSlotSurfaceSurfaces += 1;
    if (![retainedSurfaceKeys containsObject:slot.sampleSurfaceKey]) {
      [retainedSurfaceKeys addObject:slot.sampleSurfaceKey];
      retainedSampleSurfaceBytes += slot.sampleSurfaceBytes;
      retainedSampleSurfaceSurfaces += 1;
    }
  }
  if (self.latestPixelBuffer) {
    IOSurfaceRef latestSurface =
        CVPixelBufferGetIOSurface(self.latestPixelBuffer);
    uint64_t latestSurfaceBytes =
        latestSurface != NULL ? (uint64_t)IOSurfaceGetAllocSize(latestSurface)
                              : 0;
    if (latestSurface != NULL && latestSurfaceBytes > 0) {
      NSValue *latestSurfaceKey = [NSValue valueWithPointer:latestSurface];
      retainedLatestPixelBufferSurfaceBytes = latestSurfaceBytes;
      retainedLatestPixelBufferSurfaceSurfaces = 1;
      if (![retainedSurfaceKeys containsObject:latestSurfaceKey]) {
        [retainedSurfaceKeys addObject:latestSurfaceKey];
        retainedSampleSurfaceBytes += latestSurfaceBytes;
        retainedSampleSurfaceSurfaces += 1;
      }
    }
  }
  outSnapshot->capture_total_ms = self.lastCaptureTotalMs;
  outSnapshot->capture_sample_setup_ms = self.lastCaptureSampleSetupMs;
  outSnapshot->capture_lock_ms = self.lastCaptureLockMs;
  outSnapshot->capture_texture_bridge_ms = self.lastCaptureTextureBridgeMs;
  outSnapshot->capture_publish_ms = self.lastCapturePublishMs;
  outSnapshot->capture_publish_lock_ms = self.lastCapturePublishLockMs;
  outSnapshot->capture_publish_texture_refs_ms =
      self.lastCapturePublishTextureRefsMs;
  outSnapshot->capture_publish_pixel_buffer_ms =
      self.lastCapturePublishPixelBufferMs;
  outSnapshot->capture_frame_delivery_ms = self.lastCaptureFrameDeliveryMs;
  outSnapshot->sample_delivery_pool_bytes = self.observedSampleSurfacePoolBytes;
  outSnapshot->sample_delivery_pool_surfaces =
      (uint32_t)MIN(self.observedSampleSurfaceBytes.count, UINT32_MAX);
  outSnapshot->active_sample_surface_bytes = self.activeSampleSurfaceBytes;
  outSnapshot->active_sample_surface_surfaces =
      (uint32_t)MIN(self.activeSampleSurfaceRefCounts.count, UINT32_MAX);
  outSnapshot->active_sample_buffers = self.activeSampleBufferCount;
  outSnapshot->peak_active_sample_surface_bytes =
      self.peakActiveSampleSurfaceBytes;
  outSnapshot->peak_active_sample_surface_surfaces =
      self.peakActiveSampleSurfaceSurfaces;
  outSnapshot->peak_active_sample_buffers = self.peakActiveSampleBufferCount;
  outSnapshot->sample_delivery_total_samples =
      self.observedSampleSurfaceFrameCount;
  outSnapshot->sample_delivery_reused_frames =
      self.observedSampleSurfaceReusedFrames;
  outSnapshot->sample_delivery_reused_surfaces =
      (uint32_t)MIN(self.observedReusedSampleSurfaceKeys.count, UINT32_MAX);
  outSnapshot->sample_delivery_max_reuse_gap_frames =
      self.observedSampleSurfaceMaxReuseGapFrames;
  outSnapshot->retained_sample_surface_bytes = retainedSampleSurfaceBytes;
  outSnapshot->retained_sample_surface_surfaces = retainedSampleSurfaceSurfaces;
  outSnapshot->retained_published_slot_surface_bytes =
      retainedPublishedSlotSurfaceBytes;
  outSnapshot->retained_published_slot_surfaces =
      retainedPublishedSlotSurfaceSurfaces;
  outSnapshot->retained_latest_pixel_buffer_surface_bytes =
      retainedLatestPixelBufferSurfaceBytes;
  outSnapshot->retained_latest_pixel_buffer_surface_surfaces =
      retainedLatestPixelBufferSurfaceSurfaces;
  outSnapshot->latest_published_generation = [self latestPublishedGeneration];
  outSnapshot->latest_published_timestamp_ns =
      [self latestPublishedTimestampNs];
  outSnapshot->latest_presented_generation = self.latestPresentedGeneration;
  outSnapshot->generation_advances = self.observedGenerationAdvances;
  outSnapshot->samples_received = self.observedSamplesReceived;
  outSnapshot->samples_dropped_prebridge = self.observedSamplesDroppedPrebridge;
  outSnapshot->samples_bridged = self.observedSamplesBridged;
  outSnapshot->samples_published = self.observedSamplesPublished;
  outSnapshot->samples_presented = self.observedSamplesPresented;
  outSnapshot->samples_superseded_before_present =
      self.observedSamplesSupersededBeforePresent;
  os_unfair_lock_unlock(&_textureLock);
}

- (void)resetPerfCounters {
  os_unfair_lock_lock(&_textureLock);
  [self.observedSampleSurfaceBytes removeAllObjects];
  [self.observedSampleSurfaceLastSeenFrame removeAllObjects];
  [self.observedReusedSampleSurfaceKeys removeAllObjects];
  self.observedSampleSurfacePoolBytes = 0;
  self.observedSampleSurfaceFrameCount = 0;
  self.observedSampleSurfaceReusedFrames = 0;
  self.observedSampleSurfaceMaxReuseGapFrames = 0;
  self.observedGenerationAdvances = 0;
  self.observedSamplesReceived = 0;
  self.observedSamplesDroppedPrebridge = 0;
  self.observedSamplesBridged = 0;
  self.observedSamplesPublished = 0;
  self.observedSamplesPresented = 0;
  self.observedSamplesSupersededBeforePresent = 0;
  self.peakActiveSampleSurfaceBytes = self.activeSampleSurfaceBytes;
  self.peakActiveSampleSurfaceSurfaces =
      (uint32_t)MIN(self.activeSampleSurfaceRefCounts.count, UINT32_MAX);
  self.peakActiveSampleBufferCount = self.activeSampleBufferCount;
  os_unfair_lock_unlock(&_textureLock);
}

- (void)trackActiveSampleBuffer:(CMSampleBufferRef)sampleBuffer
                  sampleSurface:(NSValue *)sampleSurfaceKey
             sampleSurfaceBytes:(uint64_t)sampleSurfaceBytes {
  if (sampleBuffer == NULL) {
    return;
  }
  os_unfair_lock_lock(&_textureLock);
  self.activeSampleBufferCount = self.activeSampleBufferCount + 1;
  if (sampleSurfaceKey != nil && sampleSurfaceBytes > 0) {
    NSNumber *activeRefCount =
        self.activeSampleSurfaceRefCounts[sampleSurfaceKey];
    if (activeRefCount == nil) {
      self.activeSampleSurfaceRefCounts[sampleSurfaceKey] = @1;
      self.activeSampleSurfaceBytes =
          self.activeSampleSurfaceBytes + sampleSurfaceBytes;
    } else {
      self.activeSampleSurfaceRefCounts[sampleSurfaceKey] =
          @(activeRefCount.unsignedIntValue + 1);
    }
  }
  uint32_t activeSampleSurfaceSurfaces =
      (uint32_t)MIN(self.activeSampleSurfaceRefCounts.count, UINT32_MAX);
  if (self.activeSampleSurfaceBytes > self.peakActiveSampleSurfaceBytes) {
    self.peakActiveSampleSurfaceBytes = self.activeSampleSurfaceBytes;
  }
  if (activeSampleSurfaceSurfaces > self.peakActiveSampleSurfaceSurfaces) {
    self.peakActiveSampleSurfaceSurfaces = activeSampleSurfaceSurfaces;
  }
  if (self.activeSampleBufferCount > self.peakActiveSampleBufferCount) {
    self.peakActiveSampleBufferCount = self.activeSampleBufferCount;
  }
  os_unfair_lock_unlock(&_textureLock);

  NametagCameraSampleLifetimeToken *token =
      [NametagCameraSampleLifetimeToken new];
  token.stream = self;
  token.sampleSurfaceKey = sampleSurfaceKey;
  token.sampleSurfaceBytes = sampleSurfaceBytes;
  CMSetAttachment(
      sampleBuffer,
      (__bridge CFStringRef)kNametagSampleBufferLifetimeAttachmentKey,
      (__bridge CFTypeRef)token, kCMAttachmentMode_ShouldNotPropagate);
}

- (void)releaseTrackedActiveSampleBufferForSurface:(NSValue *)sampleSurfaceKey
                                             bytes:
                                                 (uint64_t)sampleSurfaceBytes {
  os_unfair_lock_lock(&_textureLock);
  if (self.activeSampleBufferCount > 0) {
    self.activeSampleBufferCount = self.activeSampleBufferCount - 1;
  }
  if (sampleSurfaceKey != nil && sampleSurfaceBytes > 0) {
    NSNumber *activeRefCount =
        self.activeSampleSurfaceRefCounts[sampleSurfaceKey];
    if (activeRefCount != nil) {
      uint32_t remainingRefs = activeRefCount.unsignedIntValue;
      if (remainingRefs <= 1) {
        [self.activeSampleSurfaceRefCounts removeObjectForKey:sampleSurfaceKey];
        if (self.activeSampleSurfaceBytes >= sampleSurfaceBytes) {
          self.activeSampleSurfaceBytes =
              self.activeSampleSurfaceBytes - sampleSurfaceBytes;
        } else {
          self.activeSampleSurfaceBytes = 0;
        }
      } else {
        self.activeSampleSurfaceRefCounts[sampleSurfaceKey] =
            @(remainingRefs - 1);
      }
    }
  }
  os_unfair_lock_unlock(&_textureLock);
}

- (void)copyContractSnapshot:(struct OxideCamContractSnapshot *)outSnapshot {
  if (outSnapshot == NULL) {
    return;
  }
  outSnapshot->active_width = 0;
  outSnapshot->active_height = 0;
  outSnapshot->active_fps = 0.0f;
  outSnapshot->video_range = 0;
  outSnapshot->color_space = 0;

  AVCaptureDevice *device = self.videoInput.device;
  if (device == nil) {
    return;
  }

  CMFormatDescriptionRef description = device.activeFormat.formatDescription;
  CMVideoDimensions dimensions =
      CMVideoFormatDescriptionGetDimensions(description);
  outSnapshot->active_width = (uint32_t)MAX(dimensions.width, 0);
  outSnapshot->active_height = (uint32_t)MAX(dimensions.height, 0);

  CMTime frameDuration = device.activeVideoMinFrameDuration;
  if (frameDuration.value > 0 && frameDuration.timescale > 0) {
    outSnapshot->active_fps =
        (float)frameDuration.timescale / (float)frameDuration.value;
  } else if (self.desiredFps > 0) {
    outSnapshot->active_fps = (float)self.desiredFps;
  }

  outSnapshot->video_range = (uint32_t)MAX(self.latestVideoRange, 0);
  outSnapshot->color_space = (uint32_t)MAX(self.latestColorSpace, 0);
}

- (uint64_t)latestPublishedGeneration {
  return OxidePublishedFrameGeneration(
      atomic_load_explicit(&_publishedFrameState, memory_order_acquire));
}

- (uint64_t)latestPublishedTimestampNs {
  uint64_t state =
      atomic_load_explicit(&_publishedFrameState, memory_order_acquire);
  if (state == 0) {
    return 0;
  }
  uint32_t slotIndex = OxidePublishedFrameSlot(state);
  uint64_t generation = OxidePublishedFrameGeneration(state);
  if (generation == 0 || slotIndex >= kOxideCameraPublishedSlotCount ||
      slotIndex >= self.publishedSlots.count) {
    return 0;
  }
  NametagCameraPublishedSlot *slot = self.publishedSlots[(NSUInteger)slotIndex];
  if (slot.generation != generation) {
    return 0;
  }
  return slot.timestampNs;
}

- (BOOL)copyLatestPhotoFrame:(struct NametagPhotoFrame *)frame
                       error:(NSString **)error {
  CVPixelBufferRef pixelBuffer = NULL;
  uint64_t timestampNs = 0;
  uint16_t rotationDeg = 0;
  os_unfair_lock_lock(&_textureLock);
  if (self.latestPixelBuffer) {
    pixelBuffer = (CVPixelBufferRef)CVBufferRetain(self.latestPixelBuffer);
    timestampNs = self.latestTimestampNs;
    rotationDeg = self.latestRotationDeg;
  }
  os_unfair_lock_unlock(&_textureLock);
  if (pixelBuffer == NULL) {
    if (error) {
      *error = @"messenger camera preview frame unavailable";
    }
    return NO;
  }
  BOOL ok = nametag_photo_frame_from_pixel_buffer(pixelBuffer, timestampNs,
                                                  rotationDeg, frame, error);
  CVBufferRelease(pixelBuffer);
  return ok;
}

- (void)finishPhotoCaptureWithFrame:(const struct NametagPhotoFrame *)frame {
  NametagPhotoCallback callback = self.photoCallback;
  void *context = self.photoContext;
  self.photoCaptureInFlight = NO;
  self.photoCallback = NULL;
  self.photoContext = NULL;
  nametag_dispatch_photo_completed(callback, context, frame);
}

- (void)finishPhotoCaptureWithErrorCode:(int32_t)errorCode
                                message:(NSString *)message {
  NametagPhotoCallback callback = self.photoCallback;
  void *context = self.photoContext;
  self.photoCaptureInFlight = NO;
  self.photoCallback = NULL;
  self.photoContext = NULL;
  nametag_dispatch_photo_failure(callback, context, errorCode, message);
}

- (BOOL)capturePhotoWithOptions:(const struct NametagPhotoOptions *)options
                       callback:(NametagPhotoCallback)callback
                        context:(void *)context {
  if (options == NULL || callback == NULL) {
    return NO;
  }
  AVCaptureFlashMode requestedFlashMode = AVCaptureFlashModeOff;
  switch (options->flash_mode) {
  case 0:
    requestedFlashMode = AVCaptureFlashModeOff;
    break;
  case 1:
    requestedFlashMode = AVCaptureFlashModeOn;
    break;
  case 2:
    requestedFlashMode = AVCaptureFlashModeAuto;
    break;
  default:
    nametag_dispatch_photo_failure(callback, context, 4,
                                   @"invalid photo flash mode");
    return YES;
  }

  if (options->high_speed_from_preview &&
      requestedFlashMode == AVCaptureFlashModeOff) {
    struct NametagPhotoFrame frame = {0};
    NSString *error = nil;
    if (![self copyLatestPhotoFrame:&frame error:&error]) {
      nametag_dispatch_photo_failure(callback, context, 6, error);
      return YES;
    }
    nametag_dispatch_photo_completed(callback, context, &frame);
    nametag_photo_frame_release(&frame);
    return YES;
  }

  if (self.photoCaptureInFlight) {
    nametag_dispatch_photo_failure(callback, context, 3,
                                   @"photo capture already in flight");
    return YES;
  }
  if (!self.running || self.photoOutput == nil) {
    nametag_dispatch_photo_failure(callback, context, 2,
                                   @"photo output unavailable");
    return YES;
  }

  AVCapturePhotoSettings *settings =
      [AVCapturePhotoSettings photoSettingsWithFormat:@{
        (id)kCVPixelBufferPixelFormatTypeKey :
            @(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange)
      }];
  AVCaptureDevice *videoDevice = self.videoInput.device;
  if (requestedFlashMode != AVCaptureFlashModeOff &&
      !(videoDevice && videoDevice.hasFlash)) {
    requestedFlashMode = AVCaptureFlashModeOff;
  }
  settings.flashMode = requestedFlashMode;
  if ([settings respondsToSelector:@selector(setPhotoQualityPrioritization:)]) {
    settings.photoQualityPrioritization =
        AVCapturePhotoQualityPrioritizationQuality;
  }

  self.photoCaptureInFlight = YES;
  self.photoCallback = callback;
  self.photoContext = context;
  [self.photoOutput capturePhotoWithSettings:settings delegate:self];
  return YES;
}

- (void)captureOutput:(AVCaptureOutput *)output
    didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
           fromConnection:(AVCaptureConnection *)connection {
  @autoreleasepool {
    if (self.recorder) {
      if (output == self.videoOutput) {
        [self.recorder appendVideoSample:sampleBuffer];
      } else if (output == self.audioOutput) {
        [self.recorder appendAudioSample:sampleBuffer];
      }
    }

    if (output == self.videoOutput) {
      NAMETAG_PERF_BEGIN(captureSignpost, "camera.capture.total");
      uint64_t captureTotalTicks = NametagPerfNowTicks();
      CVImageBufferRef pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer);
      if (!pixelBuffer) {
        NAMETAG_PERF_END(captureSignpost, "camera.capture.total");
        return;
      }
      os_unfair_lock_lock(&_textureLock);
      self.observedSamplesReceived = self.observedSamplesReceived + 1;
      os_unfair_lock_unlock(&_textureLock);
      IOSurfaceRef sampleSurface = NametagPerfParkedModeCurrent()
                                       ? CVPixelBufferGetIOSurface(pixelBuffer)
                                       : NULL;
      uint64_t sampleSurfaceBytes =
          sampleSurface != NULL ? (uint64_t)IOSurfaceGetAllocSize(sampleSurface)
                                : 0;
      NSValue *sampleSurfaceKey = sampleSurface != NULL
                                      ? [NSValue valueWithPointer:sampleSurface]
                                      : nil;
      uint64_t sampleSetupTicks = NametagPerfNowTicks();
      NAMETAG_PERF_BEGIN(sampleSetupSignpost, "camera.capture.sample_setup");
      OSType pixelFormat = CVPixelBufferGetPixelFormatType(pixelBuffer);
      uint64_t timestampNs = timestamp_ns_from_sample(sampleBuffer);
      BOOL requiresCpuFrameDelivery = [self requiresCpuFrameDelivery];
      NAMETAG_PERF_END(sampleSetupSignpost, "camera.capture.sample_setup");
      float sampleSetupMs = (float)NametagPerfElapsedMs(sampleSetupTicks);
      BOOL needsCpuPlaneAccess = requiresCpuFrameDelivery;
      float captureLockMs = 0.0f;
      float frameDeliveryMs = 0.0f;
      BOOL droppedPrebridge = NO;
      os_signpost_id_t lockSignpost = OS_SIGNPOST_ID_NULL;
      if (needsCpuPlaneAccess) {
        uint64_t captureLockTicks = NametagPerfNowTicks();
        if (@available(iOS 12.0, *)) {
          os_log_t perfLog = NametagPerfSignpostLog();
          lockSignpost = os_signpost_id_generate(perfLog);
          os_signpost_interval_begin(perfLog, lockSignpost,
                                     "camera.capture.lock");
        }
        CVPixelBufferLockBaseAddress(pixelBuffer, kCVPixelBufferLock_ReadOnly);
        captureLockMs = (float)NametagPerfElapsedMs(captureLockTicks);
      }
      droppedPrebridge = [self shouldDropPreviewOnlySampleBeforeBridge];
      if (!droppedPrebridge && NametagPerfParkedModeCurrent() &&
          sampleSurfaceKey != nil && sampleSurfaceBytes > 0) {
        [self trackActiveSampleBuffer:sampleBuffer
                        sampleSurface:sampleSurfaceKey
                   sampleSurfaceBytes:sampleSurfaceBytes];
      }
      float textureBridgeMs = 0.0f;
      float publishMs = 0.0f;
      if (droppedPrebridge) {
      } else if (pixelFormat == kCVPixelFormatType_32BGRA) {
        CVMetalTextureRef bgraRef = NULL;
        int width = 0;
        int height = 0;
        BOOL copiedTexture = NO;
        uint64_t bridgeTicks = NametagPerfNowTicks();
        NAMETAG_PERF_BEGIN(bridgeSignpost, "camera.capture.texture_bridge");
        if (self.textureCache) {
          copiedTexture = nametag_copy_bgra_texture_from_pixel_buffer(
              pixelBuffer, self.textureCache, &bgraRef, &width, &height);
        }
        NAMETAG_PERF_END(bridgeSignpost, "camera.capture.texture_bridge");
        textureBridgeMs = (float)NametagPerfElapsedMs(bridgeTicks);
        if (copiedTexture) {
          os_unfair_lock_lock(&_textureLock);
          self.observedSamplesBridged = self.observedSamplesBridged + 1;
          os_unfair_lock_unlock(&_textureLock);
          uint64_t publishTicks = NametagPerfNowTicks();
          NAMETAG_PERF_BEGIN(publishSignpost, "camera.capture.publish");
          [self updateLatestTextureWithBGRA:bgraRef
                                pixelBuffer:pixelBuffer
                                  timestamp:timestampNs
                                      width:width
                                     height:height];
          NAMETAG_PERF_END(publishSignpost, "camera.capture.publish");
          publishMs = (float)NametagPerfElapsedMs(publishTicks);
        }
      } else {
        size_t widthY = CVPixelBufferGetWidthOfPlane(pixelBuffer, 0);
        size_t heightY = CVPixelBufferGetHeightOfPlane(pixelBuffer, 0);
        size_t bytesPerRowY =
            CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0);

        CVMetalTextureRef yRef = NULL;
        CVMetalTextureRef uvRef = NULL;
        int bitDepth = 8;
        int matrix = 0;
        int videoRange = 0;
        int width = 0;
        int height = 0;
        BOOL copiedTexture = NO;
        uint64_t bridgeTicks = NametagPerfNowTicks();
        NAMETAG_PERF_BEGIN(bridgeSignpost, "camera.capture.texture_bridge");
        if (self.textureCache) {
          copiedTexture = nametag_copy_nv12_textures_from_pixel_buffer(
              pixelBuffer, self.textureCache, &yRef, &uvRef, &bitDepth, &matrix,
              &videoRange, &width, &height);
        }
        NAMETAG_PERF_END(bridgeSignpost, "camera.capture.texture_bridge");
        textureBridgeMs = (float)NametagPerfElapsedMs(bridgeTicks);
        if (copiedTexture) {
          os_unfair_lock_lock(&_textureLock);
          self.observedSamplesBridged = self.observedSamplesBridged + 1;
          os_unfair_lock_unlock(&_textureLock);
          uint64_t publishTicks = NametagPerfNowTicks();
          NAMETAG_PERF_BEGIN(publishSignpost, "camera.capture.publish");
          if ([self requiresLegacyLatestTextureMirror]) {
            [self updateLatestTexturesWithY:yRef
                                         uv:uvRef
                              sampleSurface:sampleSurfaceKey
                         sampleSurfaceBytes:sampleSurfaceBytes
                                pixelBuffer:pixelBuffer
                                        bit:bitDepth
                                     matrix:matrix
                                 videoRange:videoRange
                                  timestamp:timestampNs
                                      width:width
                                     height:height];
          } else {
            [self publishPreviewOnlyTexturesWithY:yRef
                                               uv:uvRef
                                              bit:bitDepth
                                           matrix:matrix
                                       videoRange:videoRange
                                        timestamp:timestampNs
                                    sampleSurface:sampleSurfaceKey
                               sampleSurfaceBytes:sampleSurfaceBytes
                                            width:width
                                           height:height];
          }
          NAMETAG_PERF_END(publishSignpost, "camera.capture.publish");
          publishMs = (float)NametagPerfElapsedMs(publishTicks);
        }

        if (requiresCpuFrameDelivery) {
          uint64_t frameDeliveryTicks = NametagPerfNowTicks();
          NAMETAG_PERF_BEGIN(frameDeliverySignpost,
                             "camera.capture.frame_delivery");
          const uint8_t *baseY =
              CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0);
          const uint8_t *baseUV =
              CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 1);
          size_t bytesPerRowUV =
              CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 1);
          size_t heightUV = CVPixelBufferGetHeightOfPlane(pixelBuffer, 1);
          size_t yLen = bytesPerRowY * heightY;
          size_t uvLen = bytesPerRowUV * heightUV;
          size_t totalLen = yLen + uvLen;
          uint8_t *buffer = (uint8_t *)malloc(totalLen);
          if (buffer && baseY && baseUV) {
            for (size_t row = 0; row < heightY; row++) {
              memcpy(buffer + row * bytesPerRowY, baseY + row * bytesPerRowY,
                     bytesPerRowY);
            }
            uint8_t *uvDest = buffer + yLen;
            for (size_t row = 0; row < heightUV; row++) {
              memcpy(uvDest + row * bytesPerRowUV, baseUV + row * bytesPerRowUV,
                     bytesPerRowUV);
            }
            struct NametagHostCameraStreamFrame frame = {
                .width = (uint32_t)widthY,
                .height = (uint32_t)heightY,
                .bytes_per_row = (uint32_t)bytesPerRowY,
                .timestamp_ns = timestampNs,
                .data = buffer,
                .len = totalLen,
            };
            self.frameCallback(&frame, self.context);
            free(buffer);
          } else if (buffer) {
            free(buffer);
          }
          NAMETAG_PERF_END(frameDeliverySignpost,
                           "camera.capture.frame_delivery");
          frameDeliveryMs = (float)NametagPerfElapsedMs(frameDeliveryTicks);
        }
      }
      if (needsCpuPlaneAccess) {
        CVPixelBufferUnlockBaseAddress(pixelBuffer,
                                       kCVPixelBufferLock_ReadOnly);
        NAMETAG_PERF_END(lockSignpost, "camera.capture.lock");
      }
      float captureTotalMs = (float)NametagPerfElapsedMs(captureTotalTicks);
      os_unfair_lock_lock(&_textureLock);
      if (NametagPerfParkedModeCurrent()) {
        self.observedSampleSurfaceFrameCount =
            self.observedSampleSurfaceFrameCount + 1;
        if (sampleSurfaceKey != nil && sampleSurfaceBytes > 0) {
          NSNumber *lastSeenFrame =
              self.observedSampleSurfaceLastSeenFrame[sampleSurfaceKey];
          if (lastSeenFrame == nil) {
            self.observedSampleSurfaceBytes[sampleSurfaceKey] =
                @(sampleSurfaceBytes);
            self.observedSampleSurfacePoolBytes =
                self.observedSampleSurfacePoolBytes + sampleSurfaceBytes;
          } else {
            uint32_t previousFrame = (uint32_t)lastSeenFrame.unsignedIntValue;
            uint32_t currentFrame = self.observedSampleSurfaceFrameCount;
            uint32_t reuseGapFrames =
                currentFrame > previousFrame ? currentFrame - previousFrame : 0;
            if (reuseGapFrames > self.observedSampleSurfaceMaxReuseGapFrames) {
              self.observedSampleSurfaceMaxReuseGapFrames = reuseGapFrames;
            }
            self.observedSampleSurfaceReusedFrames =
                self.observedSampleSurfaceReusedFrames + 1;
            [self.observedReusedSampleSurfaceKeys addObject:sampleSurfaceKey];
          }
          self.observedSampleSurfaceLastSeenFrame[sampleSurfaceKey] =
              @(self.observedSampleSurfaceFrameCount);
        }
      }
      if (droppedPrebridge) {
        self.observedSamplesDroppedPrebridge =
            self.observedSamplesDroppedPrebridge + 1;
      }
      self.lastCaptureTotalMs = captureTotalMs;
      self.lastCaptureSampleSetupMs = sampleSetupMs;
      self.lastCaptureLockMs = captureLockMs;
      self.lastCaptureTextureBridgeMs = textureBridgeMs;
      self.lastCapturePublishMs = publishMs;
      self.lastCaptureFrameDeliveryMs = frameDeliveryMs;
      os_unfair_lock_unlock(&_textureLock);
      NAMETAG_PERF_END(captureSignpost, "camera.capture.total");
      return;
    }

    if (output == self.audioOutput && self.audioCallback) {
      CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sampleBuffer);
      CMFormatDescriptionRef fmt =
          CMSampleBufferGetFormatDescription(sampleBuffer);
      const AudioStreamBasicDescription *asbd =
          fmt ? CMAudioFormatDescriptionGetStreamBasicDescription(fmt) : NULL;
      if (!block || !asbd) {
        return;
      }
      size_t length = CMBlockBufferGetDataLength(block);
      if (length == 0) {
        return;
      }
      int16_t *audio = (int16_t *)malloc(length);
      if (!audio) {
        return;
      }
      if (CMBlockBufferCopyDataBytes(block, 0, length, audio) !=
          kCMBlockBufferNoErr) {
        free(audio);
        return;
      }
      struct NametagHostAudioSample sample = {
          .channels = (uint32_t)asbd->mChannelsPerFrame,
          .sample_rate_hz = (uint32_t)asbd->mSampleRate,
          .timestamp_ns = timestamp_ns_from_sample(sampleBuffer),
          .data = audio,
          .sample_count = length / sizeof(int16_t),
      };
      self.audioCallback(&sample, self.context);
      free(audio);
    }
  }
}

- (void)photoOutput:(AVCapturePhotoOutput *)output
    didFinishProcessingPhoto:(AVCapturePhoto *)photo
                       error:(NSError *)error {
  dispatch_async(dispatch_get_main_queue(), ^{
    if (!self.photoCaptureInFlight || output != self.photoOutput) {
      return;
    }
    if (error != nil) {
      [self finishPhotoCaptureWithErrorCode:6
                                    message:error.localizedDescription];
      return;
    }
    CVPixelBufferRef pixelBuffer = photo.pixelBuffer;
    if (pixelBuffer == NULL) {
      pixelBuffer = photo.previewPixelBuffer;
    }
    if (pixelBuffer == NULL) {
      [self finishPhotoCaptureWithErrorCode:6
                                    message:@"photo capture pixel buffer "
                                            @"unavailable"];
      return;
    }
    uint64_t timestampNs = 0;
    uint16_t rotationDeg = 0;
    os_unfair_lock_lock(&self->_textureLock);
    timestampNs = self.latestTimestampNs;
    rotationDeg = self.latestRotationDeg;
    os_unfair_lock_unlock(&self->_textureLock);
    struct NametagPhotoFrame frame = {0};
    NSString *frameError = nil;
    if (!nametag_photo_frame_from_pixel_buffer(
            pixelBuffer, timestampNs, rotationDeg, &frame, &frameError)) {
      [self finishPhotoCaptureWithErrorCode:6 message:frameError];
      return;
    }
    [self finishPhotoCaptureWithFrame:&frame];
    nametag_photo_frame_release(&frame);
  });
}

@end

// ==== Registry helpers
// ========================================================

static StreamHandle RegisterStream(NametagCameraStream *stream) {
  void *ptr = (__bridge void *)stream;
  NSValue *key = CameraKeyFromPointer(ptr);
  dispatch_sync(CameraRegistryQueue(), ^{
    CameraRegistry()[key] = stream;
  });
  return (StreamHandle)ptr;
}

static NametagCameraStream *StreamForHandle(StreamHandle handle) {
  if (handle == 0) {
    return nil;
  }
  __block NametagCameraStream *stream = nil;
  NSValue *key = CameraKeyFromPointer((void *)handle);
  dispatch_sync(CameraRegistryQueue(), ^{
    stream = CameraRegistry()[key];
  });
  return stream;
}

static void UnregisterStream(StreamHandle handle) {
  if (handle == 0) {
    return;
  }
  NSValue *key = CameraKeyFromPointer((void *)handle);
  dispatch_sync(CameraRegistryQueue(), ^{
    [CameraRegistry() removeObjectForKey:key];
  });
}

// ==== Public C API
// ============================================================

StreamHandle
nametag_ios_camera_start_stream(const struct NametagHostCameraConfig *cfg,
                                NametagFrameCallback frame_cb,
                                NametagAudioCallback audio_cb, void *ctx) {
  if (cfg == NULL) {
    return 0;
  }
  __block StreamHandle handle = 0;
  NametagDispatchSyncMainQueue(^{
    NametagCameraStream *stream =
        [[NametagCameraStream alloc] initWithConfig:cfg
                                         frameBlock:frame_cb
                                         audioBlock:audio_cb
                                            context:ctx];
    if (!stream) {
      return;
    }
    if (![stream start]) {
      return;
    }
    handle = RegisterStream(stream);
  });
  return handle;
}

void nametag_ios_camera_stop_stream(StreamHandle handle) {
  NametagCameraStream *stream = StreamForHandle(handle);
  if (!stream) {
    return;
  }
  UnregisterStream(handle);
  NametagDispatchSyncMainQueue(^{
    [stream stop];
  });
}

int32_t nametag_ios_camera_select_device(int32_t device) {
  __block int32_t result = 0;
  dispatch_sync(CameraRegistryQueue(), ^{
    AVCaptureDevicePosition position = (device == 0)
                                           ? AVCaptureDevicePositionFront
                                           : AVCaptureDevicePositionBack;
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      stream.desiredPosition = position;
      if ([stream applyConfiguration]) {
        result =
            1; // At least one stream successfully applied the configuration
      } else {
        NSLog(@"[ERROR] Failed to apply device selection configuration to "
              @"stream: %@",
              stream);
      }
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_fps(uint32_t fps) {
  __block int32_t result = 0;
  dispatch_sync(CameraRegistryQueue(), ^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      stream.desiredFps = fps;
      if ([stream applyConfiguration]) {
        result =
            1; // At least one stream successfully applied the configuration
      } else {
        NSLog(@"[ERROR] Failed to apply FPS configuration to stream: %@",
              stream);
      }
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_resolution(uint32_t width, uint32_t height) {
  __block int32_t result = 0;
  dispatch_sync(CameraRegistryQueue(), ^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      stream.desiredWidth = width;
      stream.desiredHeight = height;
      if ([stream applyConfiguration]) {
        result =
            1; // At least one stream successfully applied the configuration
      } else {
        NSLog(@"[ERROR] Failed to apply resolution configuration to stream: "
              @"%@",
              stream);
      }
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_mode(int32_t mode) {
  __block int32_t result = 0;
  dispatch_sync(CameraRegistryQueue(), ^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      stream.captureMode = mode;
      if ([stream applyConfiguration]) {
        result =
            1; // At least one stream successfully applied the configuration
      } else {
        NSLog(@"[ERROR] Failed to apply mode configuration to stream: %@",
              stream);
      }
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_focus_point(float x, float y) {
  __block int32_t result = 0;
  const CGFloat clampedX = (CGFloat)fmaxf(0.0f, fminf(1.0f, x));
  const CGFloat clampedY = (CGFloat)fmaxf(0.0f, fminf(1.0f, y));
  const CGPoint focusPoint = CGPointMake(clampedX, clampedY);
  NametagDispatchSyncMainQueue(^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      AVCaptureDevice *device = stream.videoInput.device;
      if (device == nil) {
        continue;
      }
      NSError *error = nil;
      if (![device lockForConfiguration:&error]) {
        NSLog(@"[ERROR] Failed to lock camera for focus update: %@", error);
        continue;
      }
      BOOL applied = NO;
      if (device.isFocusPointOfInterestSupported &&
          [device isFocusModeSupported:AVCaptureFocusModeContinuousAutoFocus]) {
        device.focusPointOfInterest = focusPoint;
        device.focusMode = AVCaptureFocusModeContinuousAutoFocus;
        applied = YES;
      }
      if (device.isExposurePointOfInterestSupported &&
          [device isExposureModeSupported:
                      AVCaptureExposureModeContinuousAutoExposure]) {
        device.exposurePointOfInterest = focusPoint;
        device.exposureMode = AVCaptureExposureModeContinuousAutoExposure;
        applied = YES;
      }
      [device unlockForConfiguration];
      if (applied) {
        result = 1;
      }
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_zoom_factor(float factor) {
  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      AVCaptureDevice *device = stream.videoInput.device;
      if (device == nil) {
        continue;
      }
      NSError *error = nil;
      if (![device lockForConfiguration:&error]) {
        NSLog(@"[ERROR] Failed to lock camera for zoom update: %@", error);
        continue;
      }
      CGFloat minZoom = MAX((CGFloat)1.0, device.minAvailableVideoZoomFactor);
      CGFloat maxZoom = device.maxAvailableVideoZoomFactor;
      if (maxZoom < minZoom) {
        maxZoom = device.activeFormat.videoMaxZoomFactor;
      }
      if (maxZoom < minZoom) {
        maxZoom = minZoom;
      }
      CGFloat requested = (CGFloat)factor;
      if (!isfinite(requested) || requested <= 0.0) {
        requested = minZoom;
      }
      CGFloat clamped = MIN(MAX(requested, minZoom), maxZoom);
      device.videoZoomFactor = clamped;
      [device unlockForConfiguration];
      result = 1;
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_flash_mode(int32_t mode) {
  __block int32_t result = 0;
  AVCaptureFlashMode flashMode = AVCaptureFlashModeOff;
  switch (mode) {
  case 0:
    flashMode = AVCaptureFlashModeOff;
    break;
  case 1:
    flashMode = AVCaptureFlashModeOn;
    break;
  case 2:
    flashMode = AVCaptureFlashModeAuto;
    break;
  default:
    return 0;
  }
  NametagDispatchSyncMainQueue(^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      stream.flashMode = flashMode;
      result = 1;
    }
  });
  return result;
}

int32_t nametag_ios_camera_set_torch_mode(int32_t mode, float level) {
  __block int32_t result = 0;
  BOOL enabled = NO;
  switch (mode) {
  case 0:
    enabled = NO;
    break;
  case 1:
    enabled = YES;
    break;
  default:
    return 0;
  }
  NametagDispatchSyncMainQueue(^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      if ([stream applyTorchModeEnabled:enabled level:(CGFloat)level]) {
        result = 1;
      }
    }
  });
  return result;
}

int32_t
nametag_ios_camera_capture_photo(StreamHandle handle,
                                 const struct NametagPhotoOptions *options,
                                 NametagPhotoCallback event_cb, void *ctx) {
  if (options == NULL || event_cb == NULL) {
    return 0;
  }
  NametagCameraStream *stream = StreamForHandle(handle);
  if (!stream) {
    return 0;
  }

  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    if ([stream capturePhotoWithOptions:options
                               callback:event_cb
                                context:ctx]) {
      result = 1;
    }
  });
  return result;
}

RecordingHandle nametag_ios_camera_start_recording(
    StreamHandle handle, const struct NametagRecordingOptions *options,
    NametagRecordingCallback event_cb, void *ctx) {
  if (options == NULL || event_cb == NULL) {
    return 0;
  }
  NametagCameraStream *stream = StreamForHandle(handle);
  if (!stream) {
    return 0;
  }

  __block NametagCameraRecorder *recorder = nil;
  NametagDispatchSyncMainQueue(^{
    if (stream.recorder != nil) {
      return;
    }
    if (options->include_audio && ![stream ensureRecordingAudioCapture]) {
      return;
    }
    BOOL torchEnabled = NO;
    if (stream.flashMode == AVCaptureFlashModeOn) {
      torchEnabled =
          [stream applyTorchModeEnabled:YES
                                  level:AVCaptureMaxAvailableTorchLevel];
    }
    recorder = [[NametagCameraRecorder alloc] initWithOptions:options
                                                     callback:event_cb
                                                      context:ctx];
    if (recorder != nil) {
      stream.recorder = recorder;
      stream.recordingTorchEnabled = torchEnabled;
    } else {
      if (torchEnabled) {
        [stream applyTorchModeEnabled:NO level:0.0];
      }
      [stream disableRecordingAudioCaptureIfNeeded];
    }
  });

  return (RecordingHandle)(__bridge void *)recorder;
}

void nametag_ios_camera_stop_recording(RecordingHandle recording) {
  if (recording == 0)
    return;
  NametagCameraRecorder *recorder =
      (__bridge NametagCameraRecorder *)(void *)recording;

  // We need to detach from stream first
  // Find stream that has this recorder? Or just assume caller handles it?
  // Ideally we should pass the stream handle too, but for now let's just
  // finish. The stream holds a strong reference, so we need to clear it.

  // Hack: Iterate all streams to find the one with this recorder
  NametagDispatchSyncMainQueue(^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      if (stream.recorder == recorder) {
        if (stream.recordingTorchEnabled) {
          [stream applyTorchModeEnabled:NO level:0.0];
        }
        [stream disableRecordingAudioCaptureIfNeeded];
        stream.recorder = nil;
      }
    }
  });

  [recorder finish];
}

void nametag_ios_camera_cancel_recording(RecordingHandle recording) {
  if (recording == 0)
    return;
  NametagCameraRecorder *recorder =
      (__bridge NametagCameraRecorder *)(void *)recording;

  NametagDispatchSyncMainQueue(^{
    for (NametagCameraStream *stream in [CameraRegistry() allValues]) {
      if (stream.recorder == recorder) {
        if (stream.recordingTorchEnabled) {
          [stream applyTorchModeEnabled:NO level:0.0];
        }
        [stream disableRecordingAudioCaptureIfNeeded];
        stream.recorder = nil;
      }
    }
  });

  [recorder cancel];
}

static NametagCameraStream *g_default_stream = nil;
static NametagReviewVideoPlayer *g_review_video_player = nil;
static uint32_t g_oxide_camera_default_width = 1280;
static uint32_t g_oxide_camera_default_height = 720;
static uint32_t g_oxide_camera_default_fps = 30;
static AVCaptureDevicePosition g_oxide_camera_default_position =
    AVCaptureDevicePositionBack;
static NSInteger g_oxide_camera_default_mode = 0;
static AVCaptureFlashMode g_oxide_camera_default_flash_mode =
    AVCaptureFlashModeOff;
static int32_t g_oxide_camera_default_bit_depth = 8;
static int32_t g_oxide_camera_default_color_space = 0;
static int32_t g_oxide_camera_default_preview_pixel_format = 0;
static int32_t g_oxide_camera_audio_session_mode = 0;

static int32_t oxide_cam_start_default_impl(BOOL enableAudio,
                                            NSInteger captureMode) {
  __block int32_t result = -1;
  dispatch_sync(CameraRegistryQueue(), ^{
    if (g_default_stream && g_default_stream.isRunning) {
      result = 0;
      return;
    }
    struct NametagHostCameraConfig cfg = {
        .width = g_oxide_camera_default_width,
        .height = g_oxide_camera_default_height,
        .fps = g_oxide_camera_default_fps,
        .device =
            g_oxide_camera_default_position == AVCaptureDevicePositionFront ? 0
                                                                            : 1,
        .capture_mode = (int32_t)captureMode,
        .preview_pixel_format = g_oxide_camera_default_preview_pixel_format,
        .enable_audio = enableAudio,
    };
    NametagCameraStream *stream = [[NametagCameraStream alloc]
        initWithConfig:&cfg
            frameBlock:oxide_camera_stream_callback
            audioBlock:enableAudio ? oxide_camera_audio_callback : NULL
               context:NULL];
    if (stream) {
      stream.flashMode = g_oxide_camera_default_flash_mode;
    }
    if (stream && [stream start]) {
      g_default_stream = stream;
      result = 0;
    }
  });
  return result;
}

static NametagCameraStream *RegisteredStreamForRendering(void) {
  __block NametagCameraStream *stream = nil;
  dispatch_sync(CameraRegistryQueue(), ^{
    for (id candidate in [CameraRegistry() objectEnumerator]) {
      if ([candidate isKindOfClass:[NametagCameraStream class]] &&
          ((NametagCameraStream *)candidate).isRunning) {
        stream = candidate;
        break;
      }
    }
  });
  return stream;
}

static uint32_t oxide_camera_width_for_height(uint32_t height) {
  if (height == 0) {
    return g_oxide_camera_default_width;
  }
  uint64_t scaled = ((uint64_t)height * 16ull) / 9ull;
  if (scaled == 0) {
    return 1;
  }
  if (scaled > UINT32_MAX) {
    return UINT32_MAX;
  }
  return (uint32_t)scaled;
}

static BOOL oxide_camera_build_frame_from_nametag_stream(
    const struct NametagHostCameraStreamFrame *source,
    struct OxideCamFrame *target) {
  if (source == NULL || target == NULL || source->data == NULL ||
      source->width == 0 || source->height == 0) {
    return NO;
  }
  size_t yStride = source->bytes_per_row;
  size_t yLen = yStride * source->height;
  if (source->len < yLen) {
    return NO;
  }
  size_t uvLen = source->len - yLen;
  size_t uvRows = (source->height + 1) / 2;
  size_t uvStride = uvRows > 0 ? (uvLen / uvRows) : 0;
  if (uvLen > 0 && uvStride == 0) {
    uvStride = yStride;
  }
  *target = (struct OxideCamFrame){
      .y_ptr = source->data,
      .y_len = yLen,
      .y_stride = yStride,
      .uv_ptr = source->data + yLen,
      .uv_len = uvLen,
      .uv_stride = uvStride,
      .width = (int32_t)source->width,
      .height = (int32_t)source->height,
      .timestamp_ns = source->timestamp_ns,
      .rotation_deg = 0,
      .bit_depth = 8,
      .matrix = 1,
      .video_range = 1,
  };
  return YES;
}

static BOOL
oxide_camera_build_frame_from_photo(const struct NametagPhotoFrame *source,
                                    struct OxideCamFrame *target) {
  if (source == NULL || target == NULL || source->data == NULL ||
      source->width == 0 || source->height == 0) {
    return NO;
  }
  size_t yLen = (size_t)source->y_stride * (size_t)source->height;
  if (source->len < yLen) {
    return NO;
  }
  size_t uvLen = source->len - yLen;
  *target = (struct OxideCamFrame){
      .y_ptr = source->data,
      .y_len = yLen,
      .y_stride = source->y_stride,
      .uv_ptr = source->data + yLen,
      .uv_len = uvLen,
      .uv_stride = source->uv_stride,
      .width = (int32_t)source->width,
      .height = (int32_t)source->height,
      .timestamp_ns = source->timestamp_ns,
      .rotation_deg = source->rotation_deg,
      .bit_depth = source->bit_depth,
      .matrix = source->matrix,
      .video_range = source->video_range,
  };
  return YES;
}

static void
oxide_camera_stream_callback(const struct NametagHostCameraStreamFrame *frame,
                             void *ctx) {
  (void)ctx;
  if (g_oxide_camera_frame_callback == NULL) {
    return;
  }
  struct OxideCamFrame translated = {0};
  if (!oxide_camera_build_frame_from_nametag_stream(frame, &translated)) {
    return;
  }
  g_oxide_camera_frame_callback(&translated);
}

static void
oxide_camera_audio_callback(const struct NametagHostAudioSample *sample,
                            void *ctx) {
  (void)ctx;
  if (g_oxide_camera_audio_callback == NULL || sample == NULL ||
      sample->data == NULL || sample->sample_count == 0) {
    return;
  }
  struct OxideCamAudio translated = {
      .audio_ptr = sample->data,
      .sample_count = sample->sample_count,
      .channels = sample->channels,
      .sample_rate_hz = sample->sample_rate_hz,
      .timestamp_ns = sample->timestamp_ns,
  };
  g_oxide_camera_audio_callback(&translated);
}

static void
oxide_camera_record_callback(const struct NametagRecordingEvent *event,
                             void *ctx) {
  (void)ctx;
  if (g_oxide_camera_record_callback == NULL || event == NULL) {
    return;
  }
  int32_t errorCode = 0;
  if (event->kind == 2) {
    errorCode = 7;
  }
  struct OxideCamRecordEvent translated = {
      .kind = (uint32_t)event->kind,
      .path_ptr = (const uint8_t *)event->path,
      .path_len = event->path_len,
      .duration_ns = event->duration_ns,
      .size_bytes = event->size_bytes,
      .had_audio = event->had_audio ? 1 : 0,
      .error_code = errorCode,
      .error_msg_ptr = (const uint8_t *)event->error,
      .error_msg_len = event->error_len,
  };
  g_oxide_camera_record_callback(&translated);
}

static void oxide_camera_photo_callback(const struct NametagPhotoEvent *event,
                                        void *ctx) {
  (void)ctx;
  if (g_oxide_camera_photo_callback == NULL || event == NULL) {
    return;
  }
  struct OxideCamPhotoEvent translated = {
      .kind = (uint32_t)event->kind,
      .frame = {0},
      .error_code = event->error_code,
      .error_msg_ptr = (const uint8_t *)event->error,
      .error_msg_len = event->error_len,
  };
  if (event->kind == 0 &&
      !oxide_camera_build_frame_from_photo(&event->frame, &translated.frame)) {
    translated.kind = 1;
    translated.error_code = 6;
    translated.error_msg_ptr = (const uint8_t *)"photo frame conversion failed";
    translated.error_msg_len = strlen("photo frame conversion failed");
  }
  g_oxide_camera_photo_callback(&translated);
}

static BOOL oxide_camera_apply_default_stream_configuration(void) {
  if (g_default_stream == nil) {
    return YES;
  }
  g_default_stream.desiredWidth = g_oxide_camera_default_width;
  g_default_stream.desiredHeight = g_oxide_camera_default_height;
  g_default_stream.desiredFps = g_oxide_camera_default_fps;
  g_default_stream.desiredPosition = g_oxide_camera_default_position;
  g_default_stream.captureMode = g_oxide_camera_default_mode;
  g_default_stream.previewPixelFormat =
      g_oxide_camera_default_preview_pixel_format;
  g_default_stream.flashMode = g_oxide_camera_default_flash_mode;
  if (!g_default_stream.isRunning) {
    return YES;
  }
  return [g_default_stream applyConfiguration];
}

void oxide_host_set_camera_callback(OxideCameraFrameCallback callback) {
  g_oxide_camera_frame_callback = callback;
}

void oxide_host_set_camera_audio_callback(OxideCameraAudioCallback callback) {
  g_oxide_camera_audio_callback = callback;
}

void oxide_host_set_camera_record_callback(OxideCameraRecordCallback callback) {
  g_oxide_camera_record_callback = callback;
}

void oxide_host_set_camera_photo_callback(OxideCameraPhotoCallback callback) {
  g_oxide_camera_photo_callback = callback;
}

int32_t oxide_cam_start_default(void) {
  return oxide_cam_start_default_impl(YES, g_oxide_camera_default_mode);
}

int32_t oxide_cam_start_default_preview_only(void) {
  return oxide_cam_start_default_impl(NO, 0);
}

void oxide_cam_stop(void) {
  dispatch_sync(CameraRegistryQueue(), ^{
    if (g_default_stream) {
      [g_default_stream stop];
      g_default_stream = nil;
    }
  });
}

int32_t oxide_cam_set_fps(int32_t fps) {
  if (fps <= 0) {
    return -1;
  }
  g_oxide_camera_default_fps = (uint32_t)fps;
  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    if (!oxide_camera_apply_default_stream_configuration()) {
      result = -1;
    }
  });
  return result;
}

int32_t oxide_cam_set_resolution_height(int32_t height) {
  if (height < 0) {
    return -1;
  }
  g_oxide_camera_default_height = (uint32_t)height;
  g_oxide_camera_default_width =
      oxide_camera_width_for_height(g_oxide_camera_default_height);
  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    if (!oxide_camera_apply_default_stream_configuration()) {
      result = -1;
    }
  });
  return result;
}

int32_t oxide_cam_set_bit_depth(int32_t bits) {
  g_oxide_camera_default_bit_depth = bits >= 10 ? 10 : 8;
  return 0;
}

int32_t oxide_cam_set_color_space(int32_t colorSpace) {
  g_oxide_camera_default_color_space = colorSpace;
  return 0;
}

int32_t oxide_cam_set_preview_pixel_format(int32_t pixelFormat) {
  if (pixelFormat != 0 && pixelFormat != 1) {
    return -1;
  }
  g_oxide_camera_default_preview_pixel_format = pixelFormat;
  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    if (!oxide_camera_apply_default_stream_configuration()) {
      result = -1;
    }
  });
  return result;
}

int32_t oxide_cam_set_position(int32_t position) {
  g_oxide_camera_default_position = position != 0 ? AVCaptureDevicePositionFront
                                                  : AVCaptureDevicePositionBack;
  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    if (!oxide_camera_apply_default_stream_configuration()) {
      result = -1;
    }
  });
  return result;
}

int32_t oxide_cam_set_mode(int32_t mode) {
  if (mode < 0 || mode > 2) {
    return -1;
  }
  g_oxide_camera_default_mode = mode;
  switch (mode) {
  case 1:
    g_oxide_camera_default_bit_depth = 10;
    g_oxide_camera_default_color_space = 1;
    break;
  case 2:
    g_oxide_camera_default_bit_depth = 8;
    g_oxide_camera_default_color_space = 0;
    break;
  default:
    g_oxide_camera_default_bit_depth = 8;
    g_oxide_camera_default_color_space = 0;
    break;
  }
  __block int32_t result = 0;
  NametagDispatchSyncMainQueue(^{
    if (!oxide_camera_apply_default_stream_configuration()) {
      result = -1;
    }
  });
  return result;
}

int32_t oxide_cam_set_focus_point(float x, float y) {
  __block int32_t result = -1;
  const CGFloat clampedX = (CGFloat)fmaxf(0.0f, fminf(1.0f, x));
  const CGFloat clampedY = (CGFloat)fmaxf(0.0f, fminf(1.0f, y));
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil || g_default_stream.videoInput.device == nil) {
      return;
    }
    AVCaptureDevice *device = g_default_stream.videoInput.device;
    NSError *error = nil;
    if (![device lockForConfiguration:&error]) {
      return;
    }
    CGPoint point = CGPointMake(clampedX, clampedY);
    BOOL applied = NO;
    if (device.isFocusPointOfInterestSupported &&
        [device isFocusModeSupported:AVCaptureFocusModeContinuousAutoFocus]) {
      device.focusPointOfInterest = point;
      device.focusMode = AVCaptureFocusModeContinuousAutoFocus;
      applied = YES;
    }
    if (device.isExposurePointOfInterestSupported &&
        [device isExposureModeSupported:
                    AVCaptureExposureModeContinuousAutoExposure]) {
      device.exposurePointOfInterest = point;
      device.exposureMode = AVCaptureExposureModeContinuousAutoExposure;
      applied = YES;
    }
    [device unlockForConfiguration];
    if (applied) {
      result = 0;
    }
  });
  return result;
}

int32_t oxide_cam_set_zoom_factor(float factor) {
  __block int32_t result = -1;
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil || g_default_stream.videoInput.device == nil) {
      return;
    }
    AVCaptureDevice *device = g_default_stream.videoInput.device;
    NSError *error = nil;
    if (![device lockForConfiguration:&error]) {
      return;
    }
    CGFloat minZoom = MAX((CGFloat)1.0, device.minAvailableVideoZoomFactor);
    CGFloat maxZoom = device.maxAvailableVideoZoomFactor;
    if (maxZoom < minZoom) {
      maxZoom = device.activeFormat.videoMaxZoomFactor;
    }
    if (maxZoom < minZoom) {
      maxZoom = minZoom;
    }
    CGFloat requested = (CGFloat)factor;
    if (!isfinite(requested) || requested <= 0.0) {
      requested = minZoom;
    }
    device.videoZoomFactor = MIN(MAX(requested, minZoom), maxZoom);
    [device unlockForConfiguration];
    result = 0;
  });
  return result;
}

int32_t oxide_cam_set_flash_mode(int32_t mode) {
  AVCaptureFlashMode flashMode = AVCaptureFlashModeOff;
  switch (mode) {
  case 0:
    flashMode = AVCaptureFlashModeOff;
    break;
  case 1:
    flashMode = AVCaptureFlashModeOn;
    break;
  case 2:
    flashMode = AVCaptureFlashModeAuto;
    break;
  default:
    return -1;
  }
  g_oxide_camera_default_flash_mode = flashMode;
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream != nil) {
      g_default_stream.flashMode = flashMode;
    }
  });
  return 0;
}

int32_t oxide_cam_set_torch_mode(int32_t mode, float level) {
  __block int32_t result = -1;
  BOOL enabled = NO;
  switch (mode) {
  case 0:
    enabled = NO;
    break;
  case 1:
    enabled = YES;
    break;
  default:
    return -1;
  }
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil) {
      return;
    }
    if ([g_default_stream applyTorchModeEnabled:enabled level:(CGFloat)level]) {
      result = 0;
    }
  });
  return result;
}

int32_t oxide_cam_capture_photo(uint8_t high_speed_from_preview,
                                int32_t flash_mode) {
  __block int32_t result = -1;
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil) {
      return;
    }
    struct NametagPhotoOptions options = {
        .high_speed_from_preview = high_speed_from_preview != 0,
        .flash_mode = flash_mode,
    };
    if ([g_default_stream capturePhotoWithOptions:&options
                                         callback:oxide_camera_photo_callback
                                          context:NULL]) {
      result = 0;
    }
  });
  return result;
}

int32_t oxide_cam_set_audio_session_mode(int32_t mode) {
  g_oxide_camera_audio_session_mode = mode;
  return 0;
}

int32_t oxide_cam_record_start(const uint8_t *path_ptr, size_t path_len,
                               int32_t container, uint8_t include_audio) {
  __block int32_t result = -1;
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil || !g_default_stream.isRunning) {
      return;
    }
    if (g_default_stream.recorder != nil) {
      return;
    }
    NSString *path = nil;
    if (path_ptr != NULL && path_len > 0) {
      path = [[NSString alloc] initWithBytes:path_ptr
                                      length:path_len
                                    encoding:NSUTF8StringEncoding];
    }
    if (path == nil || path.length == 0) {
      NSString *extension = container == 0 ? @"mp4" : @"mov";
      NSString *fileName =
          [NSString stringWithFormat:@"oxide_%@.%@", [[NSUUID UUID] UUIDString],
                                     extension];
      path = [NSTemporaryDirectory() stringByAppendingPathComponent:fileName];
    }
    struct NametagRecordingOptions options = {
        .output_path = [path UTF8String],
        .width = g_default_stream.desiredWidth,
        .height = g_default_stream.desiredHeight,
        .fps = g_default_stream.desiredFps,
        .bitrate = 0,
        .container = container,
        .include_audio = include_audio != 0,
    };
    if (g_oxide_camera_audio_session_mode != 0) {
      (void)g_oxide_camera_audio_session_mode;
    }
    if (options.include_audio &&
        ![g_default_stream ensureRecordingAudioCapture]) {
      return;
    }
    BOOL torchEnabled = NO;
    if (g_default_stream.flashMode == AVCaptureFlashModeOn) {
      torchEnabled = [g_default_stream
          applyTorchModeEnabled:YES
                          level:AVCaptureMaxAvailableTorchLevel];
    }
    NametagCameraRecorder *recorder = [[NametagCameraRecorder alloc]
        initWithOptions:&options
               callback:oxide_camera_record_callback
                context:NULL];
    if (recorder == nil) {
      if (torchEnabled) {
        [g_default_stream applyTorchModeEnabled:NO level:0.0];
      }
      [g_default_stream disableRecordingAudioCaptureIfNeeded];
      return;
    }
    g_default_stream.recorder = recorder;
    g_default_stream.recordingTorchEnabled = torchEnabled;
    result = 0;
  });
  return result;
}

int32_t oxide_cam_record_stop(void) {
  __block int32_t result = -1;
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil || g_default_stream.recorder == nil) {
      return;
    }
    NametagCameraRecorder *recorder = g_default_stream.recorder;
    if (g_default_stream.recordingTorchEnabled) {
      [g_default_stream applyTorchModeEnabled:NO level:0.0];
    }
    [g_default_stream disableRecordingAudioCaptureIfNeeded];
    g_default_stream.recorder = nil;
    [recorder finish];
    result = 0;
  });
  return result;
}

int32_t oxide_cam_record_cancel(void) {
  __block int32_t result = -1;
  NametagDispatchSyncMainQueue(^{
    if (g_default_stream == nil || g_default_stream.recorder == nil) {
      return;
    }
    NametagCameraRecorder *recorder = g_default_stream.recorder;
    if (g_default_stream.recordingTorchEnabled) {
      [g_default_stream applyTorchModeEnabled:NO level:0.0];
    }
    [g_default_stream disableRecordingAudioCaptureIfNeeded];
    g_default_stream.recorder = nil;
    [recorder cancel];
    result = 0;
  });
  return result;
}

int32_t oxide_cam_get_latest(void **y_tex, void **uv_tex, int32_t *w,
                             int32_t *h) {
  int32_t bitdepth = 8;
  int32_t matrix = 0;
  int32_t videoRange = 0;
  int32_t colorSpace = 0;
  return oxide_cam_get_latest_ex(y_tex, uv_tex, w, h, &bitdepth, &matrix,
                                 &videoRange, &colorSpace);
}

int32_t
oxide_cam_acquire_latest_frame_ex(uint64_t min_generation_exclusive,
                                  struct OxideCamAcquiredFrame *out_frame) {
  if (out_frame == NULL || g_review_video_player != nil) {
    return 0;
  }
  memset(out_frame, 0, sizeof(*out_frame));
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return 0;
  }
  return [stream acquireLatestFrame:out_frame
                        ifNewerThan:min_generation_exclusive]
             ? 1
             : 0;
}

void *oxide_cam_get_running_session(void) {
  if (g_review_video_player != nil) {
    return NULL;
  }
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil || !stream.isRunning || stream.session == nil) {
    return NULL;
  }
  return (__bridge void *)stream.session;
}

uint64_t oxide_cam_peek_latest_generation(void) {
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return 0;
  }
  return [stream latestPublishedGeneration];
}

uint64_t oxide_cam_peek_latest_timestamp_ns(void) {
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return 0;
  }
  return [stream latestPublishedTimestampNs];
}

void oxide_cam_release_acquired(uint32_t slot, uint64_t generation) {
  if (generation == 0 || slot >= kOxideCameraPublishedSlotCount) {
    return;
  }
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil || slot >= stream.publishedSlots.count) {
    return;
  }
  [stream releaseAcquiredSlot:slot generation:generation];
}

void oxide_cam_set_preview_publish_callback(
    OxideCameraPreviewPublishCallback callback, void *context) {
  g_oxide_camera_preview_publish_callback = callback;
  g_oxide_camera_preview_publish_context = context;
}

void oxide_cam_mark_presented_generation(uint64_t generation) {
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return;
  }
  [stream notePresentedGeneration:generation];
}

int32_t oxide_cam_get_latest_bgra(void **bgra_tex, int32_t *w, int32_t *h) {
  NAMETAG_PERF_BEGIN(signpostId, "camera.fetch.live_bgra");
  id<MTLTexture> bgraTexture = nil;
  int width = 0;
  int height = 0;
  if (g_review_video_player != nil) {
    NAMETAG_PERF_END(signpostId, "camera.fetch.live_bgra");
    return 0;
  }
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil || ![stream copyLatestTextureToBGRA:&bgraTexture
                                                  width:&width
                                                 height:&height
                                            timestampNs:NULL]) {
    NAMETAG_PERF_END(signpostId, "camera.fetch.live_bgra");
    return 0;
  }
  if (bgra_tex) {
    *bgra_tex = (__bridge_retained void *)bgraTexture;
  }
  if (w) {
    *w = width;
  }
  if (h) {
    *h = height;
  }
  NAMETAG_PERF_END(signpostId, "camera.fetch.live_bgra");
  return 1;
}

int32_t oxide_cam_query_formats(void **out_ptr, size_t *out_count) {
  if (out_ptr) {
    *out_ptr = NULL;
  }
  if (out_count) {
    *out_count = 0;
  }
  return 0;
}

int32_t oxide_cam_query_pixfmts(void **out_ptr, size_t *out_count) {
  if (out_ptr) {
    *out_ptr = NULL;
  }
  if (out_count) {
    *out_count = 0;
  }
  return 0;
}

void oxide_cam_caps_free(void *ptr) {
  if (ptr != NULL) {
    free(ptr);
  }
}

int32_t oxide_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w,
                                int32_t *h, int32_t *bitdepth, int32_t *matrix,
                                int32_t *video_range, int32_t *colorspace) {
  NAMETAG_PERF_BEGIN(signpostId, "camera.fetch.live_yuv");
  if (g_review_video_player != nil) {
    id<MTLTexture> yTexture = nil;
    id<MTLTexture> uvTexture = nil;
    int width = 0;
    int height = 0;
    int bd = 0;
    int mx = 0;
    int vr = 0;
    int cs = 0;
    uint64_t timestamp = 0;
    if (![g_review_video_player copyLatestTexturesToY:&yTexture
                                                   uv:&uvTexture
                                                width:&width
                                               height:&height
                                             bitDepth:&bd
                                               matrix:&mx
                                           videoRange:&vr
                                           colorSpace:&cs
                                          timestampNs:&timestamp]) {
      NAMETAG_PERF_END(signpostId, "camera.fetch.live_yuv");
      return 0;
    }
    if (y_tex) {
      *y_tex = (__bridge_retained void *)yTexture;
    }
    if (uv_tex) {
      *uv_tex = (__bridge_retained void *)uvTexture;
    }
    if (w) {
      *w = width;
    }
    if (h) {
      *h = height;
    }
    if (bitdepth) {
      *bitdepth = bd;
    }
    if (matrix) {
      *matrix = mx;
    }
    if (video_range) {
      *video_range = vr;
    }
    if (colorspace) {
      *colorspace = cs;
    }
    NAMETAG_PERF_END(signpostId, "camera.fetch.live_yuv");
    return 1;
  }

  struct OxideCamAcquiredFrame acquired = {0};
  if (!oxide_cam_acquire_latest_frame_ex(0, &acquired)) {
    NAMETAG_PERF_END(signpostId, "camera.fetch.live_yuv");
    return 0;
  }
  id<MTLTexture> yTexture = (__bridge id<MTLTexture>)acquired.y_tex;
  id<MTLTexture> uvTexture = (__bridge id<MTLTexture>)acquired.uv_tex;
  if (y_tex) {
    *y_tex = yTexture != nil ? (__bridge_retained void *)yTexture : NULL;
  }
  if (uv_tex) {
    *uv_tex = uvTexture != nil ? (__bridge_retained void *)uvTexture : NULL;
  }
  if (w) {
    *w = acquired.width;
  }
  if (h) {
    *h = acquired.height;
  }
  if (bitdepth) {
    *bitdepth = acquired.bit_depth;
  }
  if (matrix) {
    *matrix = acquired.matrix;
  }
  if (video_range) {
    *video_range = acquired.video_range;
  }
  if (colorspace) {
    *colorspace = acquired.color_space;
  }
  oxide_cam_release_acquired(acquired.slot, acquired.generation);
  NAMETAG_PERF_END(signpostId, "camera.fetch.live_yuv");
  return 1;
}

int32_t oxide_cam_get_perf_snapshot(struct OxideCamPerfSnapshot *out_snapshot) {
  if (out_snapshot == NULL) {
    return 0;
  }
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return 0;
  }
  [stream copyPerfSnapshot:out_snapshot];
  return 1;
}

int32_t oxide_cam_reset_perf_counters(void) {
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return 0;
  }
  [stream resetPerfCounters];
  return 1;
}

int32_t
oxide_cam_get_contract_snapshot(struct OxideCamContractSnapshot *out_snapshot) {
  if (out_snapshot == NULL) {
    return 0;
  }
  NametagCameraStream *stream = g_default_stream;
  if (stream == nil || !stream.isRunning) {
    stream = RegisteredStreamForRendering();
  }
  if (stream == nil) {
    return 0;
  }
  [stream copyContractSnapshot:out_snapshot];
  return 1;
}

int32_t nametag_ios_camera_review_video_start(const char *path_ptr,
                                              size_t path_len) {
  if (path_ptr == NULL || path_len == 0) {
    return 0;
  }
  __block int32_t started = 0;
  NametagDispatchSyncMainQueue(^{
    NSString *path = [[NSString alloc] initWithBytes:path_ptr
                                              length:path_len
                                            encoding:NSUTF8StringEncoding];
    if (path == nil || path.length == 0) {
      return;
    }
    if (g_review_video_player != nil) {
      [g_review_video_player stop];
      g_review_video_player = nil;
    }
    NametagReviewVideoPlayer *player = [[NametagReviewVideoPlayer alloc]
        initWithURL:[NSURL fileURLWithPath:path]];
    if (![player start]) {
      return;
    }
    g_review_video_player = player;
    started = 1;
  });
  return started;
}

void nametag_ios_camera_review_video_clear(void) {
  NametagDispatchSyncMainQueue(^{
    if (g_review_video_player == nil) {
      return;
    }
    [g_review_video_player stop];
    g_review_video_player = nil;
  });
}

int32_t oxide_host_power_lowpower(void) {
  return [[NSProcessInfo processInfo] isLowPowerModeEnabled] ? 1 : 0;
}

int32_t oxide_host_thermal_state(void) {
  switch ([NSProcessInfo processInfo].thermalState) {
  case NSProcessInfoThermalStateNominal:
    return 0;
  case NSProcessInfoThermalStateFair:
    return 1;
  case NSProcessInfoThermalStateSerious:
    return 2;
  case NSProcessInfoThermalStateCritical:
    return 3;
  }
  return 0;
}
