#import <CoreMotion/CoreMotion.h>
#import <Foundation/Foundation.h>
#import <stdbool.h>
#import <stdint.h>

// ===== Types =====

typedef struct {
  double pressure_pa;
  double relative_altitude_m;
  uint64_t timestamp_ms;
  uint8_t has_pressure;
  uint8_t has_relative_altitude;
} OxideMotionSample;

// ===== Globals =====

static void (*g_motion_callback)(const OxideMotionSample *) = NULL;
static CMAltimeter *g_altimeter = nil;
static NSOperationQueue *g_queue = nil;
static bool g_is_running = false;

// ===== FFI Exports =====

void oxide_host_set_motion_callback(void (*cb)(const OxideMotionSample *)) {
  g_motion_callback = cb;
}

int32_t oxide_host_motion_start(void) {
  if (![CMAltimeter isRelativeAltitudeAvailable]) {
    return 1; // Unsupported
  }

  if (!g_altimeter) {
    g_altimeter = [[CMAltimeter alloc] init];
    g_queue = [[NSOperationQueue alloc] init];
  }

  if (g_is_running) {
    return 0; // Already running
  }

  [g_altimeter
      startRelativeAltitudeUpdatesToQueue:g_queue
                              withHandler:^(CMAltitudeData *_Nullable data,
                                            NSError *_Nullable error) {
                                if (!data || error)
                                  return;

                                OxideMotionSample sample;
                                sample.timestamp_ms =
                                    (uint64_t)(data.timestamp * 1000.0);

                                if (data.pressure) {
                                  sample.has_pressure = 1;
                                  sample.pressure_pa =
                                      data.pressure.doubleValue *
                                      1000.0; // kPa -> Pa
                                } else {
                                  sample.has_pressure = 0;
                                  sample.pressure_pa = 0.0;
                                }

                                if (data.relativeAltitude) {
                                  sample.has_relative_altitude = 1;
                                  sample.relative_altitude_m =
                                      data.relativeAltitude.doubleValue;
                                } else {
                                  sample.has_relative_altitude = 0;
                                  sample.relative_altitude_m = 0.0;
                                }

                                if (g_motion_callback) {
                                  g_motion_callback(&sample);
                                }
                              }];

  g_is_running = true;
  return 0;
}

void oxide_host_motion_stop(void) {
  if (g_altimeter && g_is_running) {
    [g_altimeter stopRelativeAltitudeUpdates];
    g_is_running = false;
  }
}

uint8_t oxide_host_motion_is_active(void) { return g_is_running ? 1 : 0; }
