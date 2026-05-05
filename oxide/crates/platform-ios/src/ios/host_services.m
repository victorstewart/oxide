#import <AVFoundation/AVFoundation.h>
#import <Contacts/Contacts.h>
#import <CoreBluetooth/CoreBluetooth.h>
#import <CoreLocation/CoreLocation.h>
#import <CoreMotion/CoreMotion.h>
#import <Foundation/Foundation.h>
#import <Photos/Photos.h>
#import <UIKit/UIKit.h>
#import <UserNotifications/UserNotifications.h>
#import <dispatch/dispatch.h>
#import <stdbool.h>
#import <stdatomic.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

extern void oxide_host_emit_perm(uint32_t domain, uint32_t status)
    __attribute__((weak_import));
extern void nametag_host_update_permission(int32_t domain, int32_t status)
    __attribute__((weak_import));
extern void oxide_ble_init(void) __attribute__((weak_import));

enum {
  kOxPermStatusNotDetermined = 0,
  kOxPermStatusDenied = 1,
  kOxPermStatusLimited = 2,
  kOxPermStatusAuthorized = 3,
};

enum {
  kOxPermDomainNotifications = 0,
  kOxPermDomainLocation = 1,
  kOxPermDomainCamera = 2,
  kOxPermDomainContacts = 3,
  kOxPermDomainBluetooth = 4,
  kOxPermDomainMotion = 5,
  kOxPermDomainMicrophone = 6,
  kOxPermDomainMediaLibrary = 7,
};

static const int32_t kNametagPermissionDomainLocation = 1;
static const int32_t kNametagPermissionDomainCamera = 2;
static const int32_t kNametagPermissionDomainBluetooth = 4;
static const int32_t kNametagPermissionDomainMicrophone = 6;
static const int32_t kNametagPermissionDomainMediaLibrary = 7;
static _Atomic(uint32_t) g_media_library_cached_oxide_status =
    ATOMIC_VAR_INIT(kOxPermStatusNotDetermined);
static _Atomic(int32_t) g_media_library_cached_nametag_status =
    ATOMIC_VAR_INIT(0);

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

static NSString *string_from_utf8_bytes(const char *utf8, size_t len) {
  if (utf8 == NULL || len == 0) {
    return @"";
  }
  NSString *value = [[NSString alloc] initWithBytes:utf8
                                             length:len
                                           encoding:NSUTF8StringEncoding];
  return value ?: @"";
}

static void emit_oxide_permission_async(uint32_t domain, uint32_t status) {
  if (oxide_host_emit_perm == NULL) {
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    oxide_host_emit_perm(domain, status);
  });
}

static void emit_nametag_permission_async(int32_t domain, int32_t status) {
  if (nametag_host_update_permission == NULL) {
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    nametag_host_update_permission(domain, status);
  });
}

static void emit_location_permission_updates(void);

@interface OxideSharedLocationPermissionDelegate
    : NSObject <CLLocationManagerDelegate>
@end

@implementation OxideSharedLocationPermissionDelegate
- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager {
  (void)manager;
  emit_location_permission_updates();
}

- (void)locationManager:(CLLocationManager *)manager
    didChangeAuthorizationStatus:(CLAuthorizationStatus)status {
  (void)manager;
  (void)status;
  emit_location_permission_updates();
}
@end

static OxideSharedLocationPermissionDelegate *g_location_permission_delegate =
    nil;
static CLLocationManager *g_location_permission_manager = nil;

static CLLocationManager *ensure_location_permission_manager(void) {
  static dispatch_once_t once_token;
  dispatch_once(&once_token, ^{
    g_location_permission_delegate =
        [[OxideSharedLocationPermissionDelegate alloc] init];
    g_location_permission_manager = [[CLLocationManager alloc] init];
    g_location_permission_manager.delegate = g_location_permission_delegate;
  });
  return g_location_permission_manager;
}

static uint32_t
oxide_status_from_av_authorization(AVAuthorizationStatus status) {
  switch (status) {
  case AVAuthorizationStatusAuthorized:
    return kOxPermStatusAuthorized;
  case AVAuthorizationStatusDenied:
  case AVAuthorizationStatusRestricted:
    return kOxPermStatusDenied;
  case AVAuthorizationStatusNotDetermined:
  default:
    return kOxPermStatusNotDetermined;
  }
}

static uint32_t oxide_status_from_record_permission(
    AVAudioApplicationRecordPermission permission) {
  switch (permission) {
  case AVAudioApplicationRecordPermissionGranted:
    return kOxPermStatusAuthorized;
  case AVAudioApplicationRecordPermissionDenied:
    return kOxPermStatusDenied;
  case AVAudioApplicationRecordPermissionUndetermined:
  default:
    return kOxPermStatusNotDetermined;
  }
}

static uint32_t
oxide_status_from_photo_authorization(PHAuthorizationStatus status) {
  switch (status) {
  case PHAuthorizationStatusAuthorized:
    return kOxPermStatusAuthorized;
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
  case PHAuthorizationStatusLimited:
    return kOxPermStatusLimited;
#endif
  case PHAuthorizationStatusDenied:
  case PHAuthorizationStatusRestricted:
    return kOxPermStatusDenied;
  case PHAuthorizationStatusNotDetermined:
  default:
    return kOxPermStatusNotDetermined;
  }
}

static int32_t
nametag_status_from_photo_authorization(PHAuthorizationStatus status) {
  switch (status) {
  case PHAuthorizationStatusAuthorized:
    return 3;
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
  case PHAuthorizationStatusLimited:
    return 1;
#endif
  case PHAuthorizationStatusDenied:
  case PHAuthorizationStatusRestricted:
    return 1;
  case PHAuthorizationStatusNotDetermined:
  default:
    return 0;
  }
}

static uint32_t
oxide_status_from_contact_authorization(CNAuthorizationStatus status) {
  switch (status) {
  case CNAuthorizationStatusAuthorized:
    return kOxPermStatusAuthorized;
  case CNAuthorizationStatusDenied:
  case CNAuthorizationStatusRestricted:
    return kOxPermStatusDenied;
  case CNAuthorizationStatusNotDetermined:
  default:
    return kOxPermStatusNotDetermined;
  }
}

static uint32_t
oxide_status_from_notification_settings(UNNotificationSettings *settings) {
  switch (settings.authorizationStatus) {
  case UNAuthorizationStatusAuthorized:
    if (settings.alertSetting == UNNotificationSettingEnabled ||
        settings.badgeSetting == UNNotificationSettingEnabled ||
        settings.soundSetting == UNNotificationSettingEnabled) {
      return kOxPermStatusAuthorized;
    }
    return kOxPermStatusLimited;
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

static uint32_t oxide_notification_permission_status(void) {
  __block uint32_t status = kOxPermStatusNotDetermined;
  dispatch_semaphore_t gate = dispatch_semaphore_create(0);
  void (^query)(void) = ^{
    [[UNUserNotificationCenter currentNotificationCenter]
        getNotificationSettingsWithCompletionHandler:^(
            UNNotificationSettings *settings) {
          status = settings != nil
                       ? oxide_status_from_notification_settings(settings)
                       : kOxPermStatusNotDetermined;
          dispatch_semaphore_signal(gate);
        }];
  };
  if ([NSThread isMainThread]) {
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), query);
  } else {
    query();
  }
  dispatch_time_t deadline =
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.5 * NSEC_PER_SEC));
  if (dispatch_semaphore_wait(gate, deadline) != 0) {
    return kOxPermStatusNotDetermined;
  }
  return status;
}

static uint32_t oxide_bluetooth_permission_status(void) {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 130000
  if (@available(iOS 13.0, *)) {
    switch ([CBManager authorization]) {
    case CBManagerAuthorizationAllowedAlways:
      return kOxPermStatusAuthorized;
    case CBManagerAuthorizationDenied:
    case CBManagerAuthorizationRestricted:
      return kOxPermStatusDenied;
    case CBManagerAuthorizationNotDetermined:
    default:
      return kOxPermStatusNotDetermined;
    }
  }
#endif
  return kOxPermStatusAuthorized;
}

static int32_t nametag_bluetooth_permission_status(void) {
  return (int32_t)oxide_bluetooth_permission_status();
}

static uint32_t oxide_motion_permission_status(void) {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 110000
  if (@available(iOS 11.0, *)) {
    switch ([CMMotionActivityManager authorizationStatus]) {
    case CMAuthorizationStatusAuthorized:
      return kOxPermStatusAuthorized;
    case CMAuthorizationStatusDenied:
    case CMAuthorizationStatusRestricted:
      return kOxPermStatusDenied;
    case CMAuthorizationStatusNotDetermined:
    default:
      return kOxPermStatusNotDetermined;
    }
  }
#endif
  return kOxPermStatusAuthorized;
}

static uint32_t oxide_location_permission_status(void) {
  CLLocationManager *manager = ensure_location_permission_manager();
  if (![CLLocationManager locationServicesEnabled]) {
    return kOxPermStatusDenied;
  }
  CLAuthorizationStatus status = manager.authorizationStatus;
  switch (status) {
  case kCLAuthorizationStatusAuthorizedAlways:
  case kCLAuthorizationStatusAuthorizedWhenInUse:
    // Legacy Nametag only treated location as granted when authorization was
    // present and accuracy remained full.
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
    if (@available(iOS 14.0, *)) {
      if (manager.accuracyAuthorization ==
          CLAccuracyAuthorizationReducedAccuracy) {
        return kOxPermStatusLimited;
      }
    }
#endif
    return kOxPermStatusAuthorized;
  case kCLAuthorizationStatusDenied:
  case kCLAuthorizationStatusRestricted:
    return kOxPermStatusDenied;
  case kCLAuthorizationStatusNotDetermined:
  default:
    return kOxPermStatusNotDetermined;
  }
}

static int32_t nametag_location_permission_status(void) {
  return (int32_t)oxide_location_permission_status();
}

static uint32_t oxide_camera_permission_status(void) {
  return oxide_status_from_av_authorization(
      [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo]);
}

static int32_t nametag_camera_permission_status(void) {
  return (int32_t)oxide_camera_permission_status();
}

static uint32_t oxide_microphone_permission_status(void) {
  return oxide_status_from_record_permission(
      AVAudioApplication.sharedInstance.recordPermission);
}

static int32_t nametag_microphone_permission_status(void) {
  return (int32_t)oxide_microphone_permission_status();
}

static PHAuthorizationStatus current_photo_authorization(void) {
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
  if (@available(iOS 14.0, *)) {
    return [PHPhotoLibrary
        authorizationStatusForAccessLevel:PHAccessLevelReadWrite];
  }
#endif
  return [PHPhotoLibrary authorizationStatus];
}

static void cache_media_library_permission_status(PHAuthorizationStatus status) {
  atomic_store_explicit(&g_media_library_cached_oxide_status,
                        oxide_status_from_photo_authorization(status),
                        memory_order_relaxed);
  atomic_store_explicit(&g_media_library_cached_nametag_status,
                        nametag_status_from_photo_authorization(status),
                        memory_order_relaxed);
}

static void refresh_media_library_permission_status(void) {
  cache_media_library_permission_status(current_photo_authorization());
}

static uint32_t oxide_media_library_permission_status(void) {
  refresh_media_library_permission_status();
  return atomic_load_explicit(&g_media_library_cached_oxide_status,
                              memory_order_relaxed);
}

static int32_t nametag_media_library_permission_status(void) {
  refresh_media_library_permission_status();
  return atomic_load_explicit(&g_media_library_cached_nametag_status,
                              memory_order_relaxed);
}

static void emit_location_permission_updates(void) {
  emit_oxide_permission_async(kOxPermDomainLocation,
                              oxide_location_permission_status());
  emit_nametag_permission_async(kNametagPermissionDomainLocation,
                                nametag_location_permission_status());
}

void oxide_host_clipboard_set(const char *utf8, size_t len) {
  NSString *value = string_from_utf8_bytes(utf8, len);
  dispatch_main_sync(^{
    [UIPasteboard generalPasteboard].string = value;
  });
}

int oxide_host_clipboard_get(char **out_ptr, size_t *out_len) {
  if (out_ptr == NULL || out_len == NULL) {
    return 0;
  }
  __block NSString *value = nil;
  dispatch_main_sync(^{
    value = [UIPasteboard generalPasteboard].string ?: @"";
  });
  if (value == nil) {
    *out_ptr = NULL;
    *out_len = 0;
    return 0;
  }
  NSData *data = [value dataUsingEncoding:NSUTF8StringEncoding];
  if (data == nil) {
    *out_ptr = NULL;
    *out_len = 0;
    return 0;
  }
  void *buffer = malloc(data.length);
  if (buffer == NULL && data.length > 0) {
    *out_ptr = NULL;
    *out_len = 0;
    return 0;
  }
  if (data.length > 0) {
    memcpy(buffer, data.bytes, data.length);
  }
  *out_ptr = buffer;
  *out_len = data.length;
  return 1;
}

void oxide_host_string_free(char *p) {
  if (p != NULL) {
    free(p);
  }
}

static UIImpactFeedbackGenerator *
impact_generator(UIImpactFeedbackStyle style) {
  switch (style) {
  case UIImpactFeedbackStyleLight: {
    static UIImpactFeedbackGenerator *light = nil;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      light = [[UIImpactFeedbackGenerator alloc]
          initWithStyle:UIImpactFeedbackStyleLight];
    });
    return light;
  }
  case UIImpactFeedbackStyleMedium: {
    static UIImpactFeedbackGenerator *medium = nil;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      medium = [[UIImpactFeedbackGenerator alloc]
          initWithStyle:UIImpactFeedbackStyleMedium];
    });
    return medium;
  }
  case UIImpactFeedbackStyleHeavy: {
    static UIImpactFeedbackGenerator *heavy = nil;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      heavy = [[UIImpactFeedbackGenerator alloc]
          initWithStyle:UIImpactFeedbackStyleHeavy];
    });
    return heavy;
  }
  default:
    return nil;
  }
}

static UISelectionFeedbackGenerator *selection_generator(void) {
  static UISelectionFeedbackGenerator *selection = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    selection = [[UISelectionFeedbackGenerator alloc] init];
  });
  return selection;
}

static UINotificationFeedbackGenerator *notification_generator(void) {
  static UINotificationFeedbackGenerator *notification = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    notification = [[UINotificationFeedbackGenerator alloc] init];
  });
  return notification;
}

void oxide_host_haptics_play(uint32_t pattern) {
  dispatch_main_async(^{
    switch (pattern) {
    case 0: {
      UIImpactFeedbackGenerator *generator =
          impact_generator(UIImpactFeedbackStyleLight);
      [generator impactOccurred];
      [generator prepare];
      break;
    }
    case 1:
      [impact_generator(UIImpactFeedbackStyleMedium) impactOccurred];
      [impact_generator(UIImpactFeedbackStyleMedium) prepare];
      break;
    case 2:
      [impact_generator(UIImpactFeedbackStyleHeavy) impactOccurred];
      [impact_generator(UIImpactFeedbackStyleHeavy) prepare];
      break;
    case 3:
      [selection_generator() selectionChanged];
      [selection_generator() prepare];
      break;
    case 5:
      [notification_generator()
          notificationOccurred:UINotificationFeedbackTypeWarning];
      [notification_generator() prepare];
      break;
    case 6:
      [notification_generator()
          notificationOccurred:UINotificationFeedbackTypeError];
      [notification_generator() prepare];
      break;
    case 4:
    default:
      [notification_generator()
          notificationOccurred:UINotificationFeedbackTypeSuccess];
      [notification_generator() prepare];
      break;
    }
  });
}

uint32_t oxide_host_perm_status(uint32_t domain) {
  switch (domain) {
  case kOxPermDomainNotifications:
    return oxide_notification_permission_status();
  case kOxPermDomainLocation:
    return oxide_location_permission_status();
  case kOxPermDomainCamera:
    return oxide_camera_permission_status();
  case kOxPermDomainContacts:
    return oxide_status_from_contact_authorization(
        [CNContactStore authorizationStatusForEntityType:CNEntityTypeContacts]);
  case kOxPermDomainBluetooth:
    return oxide_bluetooth_permission_status();
  case kOxPermDomainMotion:
    return oxide_motion_permission_status();
  case kOxPermDomainMicrophone:
    return oxide_microphone_permission_status();
  case kOxPermDomainMediaLibrary:
    return oxide_media_library_permission_status();
  default:
    return kOxPermStatusDenied;
  }
}

void oxide_host_perm_request(uint32_t domain) {
  switch (domain) {
  case kOxPermDomainNotifications: {
    UNAuthorizationOptions options = UNAuthorizationOptionAlert |
                                     UNAuthorizationOptionBadge |
                                     UNAuthorizationOptionSound;
    [[UNUserNotificationCenter currentNotificationCenter]
        requestAuthorizationWithOptions:options
                      completionHandler:^(BOOL granted, NSError *error) {
                        (void)granted;
                        (void)error;
                        emit_oxide_permission_async(
                            kOxPermDomainNotifications,
                            oxide_notification_permission_status());
                      }];
    break;
  }
  case kOxPermDomainLocation:
    dispatch_main_async(^{
      [ensure_location_permission_manager() requestWhenInUseAuthorization];
    });
    emit_location_permission_updates();
    break;
  case kOxPermDomainCamera:
    [AVCaptureDevice
        requestAccessForMediaType:AVMediaTypeVideo
                completionHandler:^(BOOL granted) {
                  (void)granted;
                  emit_oxide_permission_async(kOxPermDomainCamera,
                                              oxide_camera_permission_status());
                }];
    break;
  case kOxPermDomainContacts: {
    CNContactStore *store = [CNContactStore new];
    [store requestAccessForEntityType:CNEntityTypeContacts
                    completionHandler:^(BOOL granted, NSError *error) {
                      (void)granted;
                      (void)error;
                      emit_oxide_permission_async(
                          kOxPermDomainContacts,
                          oxide_status_from_contact_authorization(
                              [CNContactStore authorizationStatusForEntityType:
                                                  CNEntityTypeContacts]));
                    }];
    break;
  }
  case kOxPermDomainBluetooth:
    if (oxide_ble_init != NULL) {
      oxide_ble_init();
    }
    emit_oxide_permission_async(kOxPermDomainBluetooth,
                                oxide_bluetooth_permission_status());
    break;
  case kOxPermDomainMotion:
    emit_oxide_permission_async(kOxPermDomainMotion,
                                oxide_motion_permission_status());
    break;
  case kOxPermDomainMicrophone:
    [AVAudioApplication
        requestRecordPermissionWithCompletionHandler:^(BOOL granted) {
          (void)granted;
          emit_oxide_permission_async(kOxPermDomainMicrophone,
                                      oxide_microphone_permission_status());
        }];
    break;
  case kOxPermDomainMediaLibrary:
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
    if (@available(iOS 14.0, *)) {
      [PHPhotoLibrary
          requestAuthorizationForAccessLevel:PHAccessLevelReadWrite
                                     handler:^(PHAuthorizationStatus status) {
                                       cache_media_library_permission_status(
                                           status);
                                       emit_oxide_permission_async(
                                           kOxPermDomainMediaLibrary,
                                           oxide_media_library_permission_status());
                                     }];
      break;
    }
#endif
    [PHPhotoLibrary requestAuthorization:^(PHAuthorizationStatus status) {
      cache_media_library_permission_status(status);
      emit_oxide_permission_async(kOxPermDomainMediaLibrary,
                                  oxide_media_library_permission_status());
    }];
    break;
  default:
    emit_oxide_permission_async(domain, kOxPermStatusDenied);
    break;
  }
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_publish_permissions(void) {
  emit_nametag_permission_async(kNametagPermissionDomainLocation,
                                nametag_location_permission_status());
  emit_nametag_permission_async(kNametagPermissionDomainBluetooth,
                                nametag_bluetooth_permission_status());
  emit_nametag_permission_async(kNametagPermissionDomainCamera,
                                nametag_camera_permission_status());
  emit_nametag_permission_async(kNametagPermissionDomainMicrophone,
                                nametag_microphone_permission_status());
  // Keep Photos permission lazy. Boot-time permission sync must not probe
  // PHPhotoLibrary before the app explicitly tries to access the library.
}

void nametag_ios_request_permission(int32_t domain) {
  switch (domain) {
  case kNametagPermissionDomainLocation:
    dispatch_main_async(^{
      [ensure_location_permission_manager() requestWhenInUseAuthorization];
    });
    emit_location_permission_updates();
    break;
  case kNametagPermissionDomainBluetooth:
    if (oxide_ble_init != NULL) {
      oxide_ble_init();
    }
    emit_nametag_permission_async(kNametagPermissionDomainBluetooth,
                                  nametag_bluetooth_permission_status());
    break;
  case kNametagPermissionDomainCamera:
    [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo
                             completionHandler:^(BOOL granted) {
                               (void)granted;
                               emit_nametag_permission_async(
                                   kNametagPermissionDomainCamera,
                                   nametag_camera_permission_status());
                             }];
    break;
  case kNametagPermissionDomainMicrophone:
    [AVAudioApplication
        requestRecordPermissionWithCompletionHandler:^(BOOL granted) {
          (void)granted;
          emit_nametag_permission_async(kNametagPermissionDomainMicrophone,
                                        nametag_microphone_permission_status());
        }];
    break;
  case kNametagPermissionDomainMediaLibrary:
#if __IPHONE_OS_VERSION_MAX_ALLOWED >= 140000
    if (@available(iOS 14.0, *)) {
      [PHPhotoLibrary
          requestAuthorizationForAccessLevel:PHAccessLevelReadWrite
                                     handler:^(PHAuthorizationStatus status) {
                                       cache_media_library_permission_status(
                                           status);
                                       emit_nametag_permission_async(
                                           kNametagPermissionDomainMediaLibrary,
                                           nametag_media_library_permission_status());
                                     }];
      break;
    }
#endif
    [PHPhotoLibrary requestAuthorization:^(PHAuthorizationStatus status) {
      cache_media_library_permission_status(status);
      emit_nametag_permission_async(kNametagPermissionDomainMediaLibrary,
                                    nametag_media_library_permission_status());
    }];
    break;
  default:
    break;
  }
}

int32_t nametag_ios_permission_status(int32_t domain) {
  switch (domain) {
  case kNametagPermissionDomainLocation:
    return nametag_location_permission_status();
  case kNametagPermissionDomainBluetooth:
    return nametag_bluetooth_permission_status();
  case kNametagPermissionDomainCamera:
    return nametag_camera_permission_status();
  case kNametagPermissionDomainMicrophone:
    return nametag_microphone_permission_status();
  case kNametagPermissionDomainMediaLibrary:
    return nametag_media_library_permission_status();
  default:
    return 0;
  }
}
#endif

void nametag_ios_clipboard_set(const char *utf8, size_t len) {
  oxide_host_clipboard_set(utf8, len);
}

void nametag_ios_open_system_settings(void) {
  dispatch_main_async(^{
    UIApplication *application = UIApplication.sharedApplication;
    NSURL *url = [NSURL URLWithString:UIApplicationOpenSettingsURLString];
    if (application == nil || url == nil || ![application canOpenURL:url]) {
      return;
    }
    [application openURL:url options:@{} completionHandler:nil];
  });
}

int32_t nametag_ios_open_external_url(const char *utf8, size_t len) {
  __block int32_t opened = 0;
  dispatch_main_sync(^{
    NSString *value = string_from_utf8_bytes(utf8, len);
    if (value.length == 0) {
      return;
    }
    NSURL *url = [NSURL URLWithString:value];
    UIApplication *application = UIApplication.sharedApplication;
    if (application == nil || url == nil || ![application canOpenURL:url]) {
      return;
    }
    [application openURL:url options:@{} completionHandler:nil];
    opened = 1;
  });
  return opened;
}

void nametag_ios_haptics_play(int32_t pattern) {
  oxide_host_haptics_play((uint32_t)MAX(pattern, 0));
}
