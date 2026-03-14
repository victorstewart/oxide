#import <CoreLocation/CoreLocation.h>
#import <Foundation/Foundation.h>
#import <stdbool.h>
#import <stdint.h>

// ===== Types =====

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

// ===== Globals =====

static void (*g_location_callback)(const OxideLocationSample *) = NULL;
static void (*g_location_error_callback)(const uint8_t *, size_t) = NULL;

static OxideLocationSample g_last_sample;
static bool g_has_last_sample = false;

// ===== Delegate =====

@interface OxideLocationDelegate : NSObject <CLLocationManagerDelegate>
@end

@implementation OxideLocationDelegate

- (void)locationManager:(CLLocationManager *)manager
     didUpdateLocations:(NSArray<CLLocation *> *)locations {
  CLLocation *latest = locations.lastObject;
  if (!latest)
    return;

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

  if (g_location_callback) {
    g_location_callback(&sample);
  }
}

- (void)locationManager:(CLLocationManager *)manager
       didFailWithError:(NSError *)error {
  if (g_location_error_callback) {
    const char *msg = error.localizedDescription.UTF8String ?: "unknown error";
    g_location_error_callback((const uint8_t *)msg, strlen(msg));
  }
}

@end

// ===== Manager =====

static CLLocationManager *g_manager = nil;
static OxideLocationDelegate *g_delegate = nil;

static CLLocationManager *ensure_manager(void) {
  if (!g_manager) {
    g_manager = [[CLLocationManager alloc] init];
    g_delegate = [[OxideLocationDelegate alloc] init];
    g_manager.delegate = g_delegate;
  }
  return g_manager;
}

// ===== FFI Exports =====

void oxide_host_set_location_callback(
    void (*cb)(const OxideLocationSample *)) {
  g_location_callback = cb;
}

void oxide_host_set_location_error_callback(void (*cb)(const uint8_t *,
                                                         size_t)) {
  g_location_error_callback = cb;
}

int32_t oxide_host_location_start(OxideLocationConfig cfg) {
  CLLocationManager *mgr = ensure_manager();

  // Accuracy
  switch (cfg.accuracy_kind) {
  case 0:
    mgr.desiredAccuracy = kCLLocationAccuracyReduced;
    break;
  case 1:
    mgr.desiredAccuracy = kCLLocationAccuracyHundredMeters;
    break;
  case 2:
    mgr.desiredAccuracy = kCLLocationAccuracyBest;
    break;
  default:
    mgr.desiredAccuracy = kCLLocationAccuracyBest;
    break;
  }

  // Filter
  mgr.distanceFilter =
      cfg.distance_filter_m > 0 ? cfg.distance_filter_m : kCLDistanceFilterNone;

  // Background
  mgr.allowsBackgroundLocationUpdates = cfg.allow_background ? YES : NO;
  mgr.showsBackgroundLocationIndicator = cfg.allow_background ? YES : NO;
  mgr.pausesLocationUpdatesAutomatically = NO;

  // Auth
  CLAuthorizationStatus status = mgr.authorizationStatus;
  if (status == kCLAuthorizationStatusNotDetermined) {
    [mgr requestWhenInUseAuthorization];
  }
  if (cfg.allow_background &&
      status != kCLAuthorizationStatusAuthorizedAlways) {
    [mgr requestAlwaysAuthorization];
  }

  [mgr startUpdatingLocation];
  return 0;
}

void oxide_host_location_stop(void) {
  if (g_manager) {
    [g_manager stopUpdatingLocation];
  }
}

void oxide_host_location_request_once(void) {
  CLLocationManager *mgr = ensure_manager();
  if (@available(iOS 9.0, *)) {
    [mgr requestLocation];
  } else {
    [mgr startUpdatingLocation];
  }
}

uint8_t oxide_host_location_last(OxideLocationSample *out_ptr) {
  if (g_has_last_sample && out_ptr) {
    *out_ptr = g_last_sample;
    return 1;
  }
  return 0;
}
