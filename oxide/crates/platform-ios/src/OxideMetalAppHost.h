#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#include <stdint.h>

typedef int32_t (*OxideMetalAppInitFn)(uint32_t width, uint32_t height, float scale);
typedef int32_t (*OxideMetalAppPrepareFrameFn)(uint32_t width, uint32_t height,
                                               float scale);
typedef int32_t (*OxideMetalAppSubmitPreparedFrameFn)(void *drawable);
typedef void (*OxideMetalAppCancelPreparedFrameFn)(void);
typedef void (*OxideMetalAppShutdownFn)(void);
typedef void (*OxideMetalAppTouchFn)(uint64_t id, uint32_t phase, float x, float y,
                                     uint64_t time_ns);

typedef struct OxideMetalAppHostConfig {
  NSString *scene_configuration_name;
  NSString *log_prefix;
  NSString *touch_log_env_name;
  NSString *touch_log_filename;
  OxideMetalAppInitFn init;
  OxideMetalAppPrepareFrameFn prepare_frame;
  OxideMetalAppSubmitPreparedFrameFn submit_prepared_frame;
  OxideMetalAppCancelPreparedFrameFn cancel_prepared_frame;
  OxideMetalAppShutdownFn shutdown;
  OxideMetalAppTouchFn touch;
} OxideMetalAppHostConfig;

const OxideMetalAppHostConfig *oxide_metal_app_host_config(void);
int oxide_metal_app_host_start(int argc, char **argv);
