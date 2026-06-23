#import <CoreLocation/CoreLocation.h>
#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#import <dispatch/dispatch.h>
#import <stdbool.h>
#import <stddef.h>
#import <stdint.h>

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

_Static_assert(sizeof(OxideLocationSample) == 64,
               "OxideLocationSample ABI size changed");
_Static_assert(_Alignof(OxideLocationSample) == 8,
               "OxideLocationSample ABI alignment changed");

typedef struct {
  uint32_t accuracy_kind;
  double distance_filter_m;
  uint8_t allow_background;
  uint8_t precise;
} OxideLocationConfig;

_Static_assert(sizeof(OxideLocationConfig) == 24,
               "OxideLocationConfig ABI size changed");
_Static_assert(_Alignof(OxideLocationConfig) == 8,
               "OxideLocationConfig ABI alignment changed");

static void (*g_location_callback)(const OxideLocationSample *) = NULL;
static void (*g_location_error_callback)(const uint8_t *, size_t) = NULL;

static OxideLocationSample g_last_sample;
static bool g_has_last_sample = false;

static void dispatch_main_async(void (^block)(void)) {
  if ([NSThread isMainThread]) {
    block();
    return;
  }
  dispatch_async(dispatch_get_main_queue(), block);
}

static void dispatch_main_sync(void (^block)(void)) {
  if ([NSThread isMainThread]) {
    block();
    return;
  }
  dispatch_sync(dispatch_get_main_queue(), block);
}

static CLLocationManager *ensure_manager(void);
static void apply_location_config(CLLocationManager *manager,
                                  OxideLocationConfig cfg);
static CLLocationAccuracy desired_accuracy_for_kind(uint32_t accuracy_kind);

@interface OxideLocationDelegate : NSObject <CLLocationManagerDelegate>
@end

@implementation OxideLocationDelegate

- (void)locationManager:(CLLocationManager *)manager
     didUpdateLocations:(NSArray<CLLocation *> *)locations {
  (void)manager;
  CLLocation *latest = locations.lastObject;
  if (latest == nil) {
    return;
  }

  OxideLocationSample sample;
  sample.latitude = latest.coordinate.latitude;
  sample.longitude = latest.coordinate.longitude;
  sample.altitude = latest.altitude;
  sample.horizontal_accuracy = latest.horizontalAccuracy;
  sample.vertical_accuracy = latest.verticalAccuracy;
  sample.speed = latest.speed < 0 ? 0 : latest.speed;
  sample.course = latest.course < 0 ? 0 : latest.course;
  sample.timestamp_ms =
      (uint64_t)(latest.timestamp.timeIntervalSince1970 * 1000.0);

  g_last_sample = sample;
  g_has_last_sample = true;

  if (g_location_callback != NULL) {
    g_location_callback(&sample);
  }
}

- (void)locationManager:(CLLocationManager *)manager
       didFailWithError:(NSError *)error {
  (void)manager;
  if (g_location_error_callback == NULL) {
    return;
  }
  const char *msg = error.localizedDescription.UTF8String ?: "unknown error";
  g_location_error_callback((const uint8_t *)msg, strlen(msg));
}

@end

static CLLocationManager *g_manager = nil;
static OxideLocationDelegate *g_delegate = nil;

static CLLocationManager *ensure_manager(void) {
  __block CLLocationManager *manager = nil;
  dispatch_main_sync(^{
    if (g_manager == nil) {
      g_manager = [[CLLocationManager alloc] init];
      g_delegate = [[OxideLocationDelegate alloc] init];
      g_manager.delegate = g_delegate;
    }
    manager = g_manager;
  });
  return manager;
}

static void apply_location_config(CLLocationManager *manager,
                                  OxideLocationConfig cfg) {
  (void)cfg.precise;
  manager.desiredAccuracy = desired_accuracy_for_kind(cfg.accuracy_kind);

  manager.activityType = CLActivityTypeOther;
  manager.pausesLocationUpdatesAutomatically = NO;
  manager.distanceFilter =
      cfg.distance_filter_m > 0 ? cfg.distance_filter_m : kCLDistanceFilterNone;
  manager.allowsBackgroundLocationUpdates = cfg.allow_background ? YES : NO;
}

static CLLocationAccuracy desired_accuracy_for_kind(uint32_t accuracy_kind) {
  switch (accuracy_kind) {
  case 0:
    return kCLLocationAccuracyReduced;
  case 1:
    return kCLLocationAccuracyHundredMeters;
  case 2:
    return kCLLocationAccuracyThreeKilometers;
  case 3:
    return kCLLocationAccuracyBest;
  default:
    return kCLLocationAccuracyBest;
  }
}

void oxide_host_set_location_callback(void (*cb)(const OxideLocationSample *)) {
  g_location_callback = cb;
}

void oxide_host_set_location_error_callback(void (*cb)(const uint8_t *,
                                                       size_t)) {
  g_location_error_callback = cb;
}

int32_t oxide_host_location_start(OxideLocationConfig cfg) {
  dispatch_main_sync(^{
    CLLocationManager *manager = ensure_manager();
    apply_location_config(manager, cfg);

    CLAuthorizationStatus status = manager.authorizationStatus;
    if (status == kCLAuthorizationStatusNotDetermined) {
      [manager requestWhenInUseAuthorization];
    }
    if (cfg.allow_background &&
        status != kCLAuthorizationStatusAuthorizedAlways) {
      [manager requestAlwaysAuthorization];
    }

    [manager startUpdatingLocation];
  });
  return 0;
}

void oxide_host_location_stop(void) {
  dispatch_main_async(^{
    if (g_manager != nil) {
      [g_manager stopUpdatingLocation];
    }
  });
}

void oxide_host_location_request_once(void) {
  dispatch_main_async(^{
    CLLocationManager *manager = ensure_manager();
    [manager requestLocation];
  });
}

uint8_t oxide_host_location_last(OxideLocationSample *out_ptr) {
  if (out_ptr == NULL) {
    return 0;
  }
  __block uint8_t has_sample = 0;
  dispatch_main_sync(^{
    if (!g_has_last_sample) {
      return;
    }
    *out_ptr = g_last_sample;
    has_sample = 1;
  });
  return has_sample;
}

int32_t oxide_host_location_set_accuracy(uint32_t accuracy_kind) {
  dispatch_main_sync(^{
    CLLocationManager *manager = ensure_manager();
    manager.desiredAccuracy = desired_accuracy_for_kind(accuracy_kind);
  });
  return 0;
}
