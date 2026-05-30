#import <Foundation/Foundation.h>
#import <TargetConditionals.h>
#import <UIKit/UIKit.h>
#import <UserNotifications/UserNotifications.h>
#import <dispatch/dispatch.h>
#import <stdbool.h>
#import <stdlib.h>
#import <strings.h>

extern void oxide_host_emit_perm(uint32_t domain, uint32_t status)
    __attribute__((weak_import));
extern void oxide_host_emit_push_token(uint32_t provider, const char *utf8,
                                       size_t len)
    __attribute__((weak_import));
extern void oxide_host_emit_push_notify(const char *utf8, size_t len)
    __attribute__((weak_import));
#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
extern void nametag_host_update_permission(int32_t domain, int32_t status)
    __attribute__((weak_import));
extern void nametag_ios_handle_notification_response(NSDictionary *userInfo)
    __attribute__((weak_import));
#endif

static const int32_t kPermissionDomainNotifications = 0;
static const uint32_t kOxidePushProviderApns = 0;

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
struct NametagHostUtf8 {
  const uint8_t *ptr;
  size_t len;
};

struct NametagPushField {
  struct NametagHostUtf8 key;
  struct NametagHostUtf8 value;
};

struct NametagPushNotification {
  const struct NametagPushField *fields;
  size_t field_count;
  struct NametagHostUtf8 sound;
  bool has_badge;
  int32_t badge;
};

typedef void (*NametagPushEventCallback)(int32_t kind, const void *payload,
                                         void *ctx);

static const int32_t NAMETAG_PUSH_EVENT_TOKEN = 0;
static const int32_t NAMETAG_PUSH_EVENT_NOTIFICATION = 1;

static NametagPushEventCallback g_push_callback = NULL;
static void *g_push_context = NULL;
#endif

static bool env_bool(const char *name, bool fallback) {
  const char *value = getenv(name);
  if (value == NULL) {
    return fallback;
  }
  if (strcasecmp(value, "1") == 0 || strcasecmp(value, "true") == 0 ||
      strcasecmp(value, "on") == 0 || strcasecmp(value, "yes") == 0) {
    return true;
  }
  if (strcasecmp(value, "0") == 0 || strcasecmp(value, "false") == 0 ||
      strcasecmp(value, "off") == 0 || strcasecmp(value, "no") == 0) {
    return false;
  }
  return fallback;
}

static bool suppress_push_prompt(void) {
#if TARGET_OS_SIMULATOR
  return !env_bool("NAMETAG_HOST_ALLOW_PUSH_PROMPT", false);
#else
  return env_bool("NAMETAG_DISABLE_PUSH_PROMPT", false);
#endif
}

static dispatch_queue_t push_queue(void) {
  static dispatch_queue_t queue;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    queue = dispatch_queue_create("com.oxide.platform.push",
                                  DISPATCH_QUEUE_SERIAL);
  });
  return queue;
}

static int32_t notification_status_code(UNAuthorizationStatus status) {
  switch (status) {
  case UNAuthorizationStatusDenied:
    return 1;
  case UNAuthorizationStatusAuthorized:
    return 3;
#if defined(UNAuthorizationStatusProvisional)
  case UNAuthorizationStatusProvisional:
    return 2;
#endif
#if defined(UNAuthorizationStatusEphemeral)
  case UNAuthorizationStatusEphemeral:
    return 2;
#endif
  case UNAuthorizationStatusNotDetermined:
  default:
    return 0;
  }
}

static void publish_notification_permission(UNAuthorizationStatus status) {
  int32_t code = notification_status_code(status);
#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
  if (nametag_host_update_permission != NULL) {
    nametag_host_update_permission(kPermissionDomainNotifications, code);
  }
#endif
  if (oxide_host_emit_perm != NULL) {
    oxide_host_emit_perm((uint32_t)kPermissionDomainNotifications,
                         (uint32_t)code);
  }
}

static void emit_oxide_token_event(NSString *token) {
  if (oxide_host_emit_push_token == NULL) {
    return;
  }
  NSData *utf8 = [token dataUsingEncoding:NSUTF8StringEncoding];
  oxide_host_emit_push_token(kOxidePushProviderApns, utf8.bytes, utf8.length);
}

static void emit_token_event(NSString *token) {
#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
  if (g_push_callback != NULL) {
    NSData *utf8 = [token dataUsingEncoding:NSUTF8StringEncoding];
    struct NametagHostUtf8 payload;
    payload.ptr = (const uint8_t *)utf8.bytes;
    payload.len = utf8.length;
    g_push_callback(NAMETAG_PUSH_EVENT_TOKEN, &payload, g_push_context);
  }
#endif
  emit_oxide_token_event(token);
}

static NSString *string_from_data(NSData *data) {
  if (data.length == 0) {
    return @"";
  }
  const unsigned char *bytes = data.bytes;
  NSMutableString *string =
      [[NSMutableString alloc] initWithCapacity:data.length * 2];
  for (NSUInteger index = 0; index < data.length; index++) {
    [string appendFormat:@"%02x", bytes[index]];
  }
  return string;
}

static NSString *json_string_from_notification(NSDictionary *userInfo) {
  if (![NSJSONSerialization isValidJSONObject:userInfo]) {
    return nil;
  }
  NSError *error = nil;
  NSData *json =
      [NSJSONSerialization dataWithJSONObject:userInfo options:0 error:&error];
  if (json.length == 0 || error != nil) {
    return nil;
  }
  return [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
}

static void emit_oxide_notification_event(NSDictionary *userInfo) {
  if (oxide_host_emit_push_notify == NULL) {
    return;
  }
  NSString *json = json_string_from_notification(userInfo ?: @{});
  if (json.length == 0) {
    return;
  }
  NSData *utf8 = [json dataUsingEncoding:NSUTF8StringEncoding];
  oxide_host_emit_push_notify((const char *)utf8.bytes, utf8.length);
}

static void emit_notification_event(NSDictionary *userInfo) {
  emit_oxide_notification_event(userInfo);

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
  if (g_push_callback == NULL) {
    return;
  }

  NSDictionary *aps = userInfo[@"aps"];
  NSNumber *badgeNumber = nil;
  NSString *soundName = nil;
  if ([aps isKindOfClass:[NSDictionary class]]) {
    badgeNumber = aps[@"badge"];
    id sound = aps[@"sound"];
    if ([sound isKindOfClass:[NSString class]]) {
      soundName = sound;
    }
  }

  NSMutableArray<NSString *> *keys = [[NSMutableArray alloc] init];
  NSMutableArray<NSString *> *values = [[NSMutableArray alloc] init];
  [userInfo enumerateKeysAndObjectsUsingBlock:^(id key, id obj, BOOL *stop) {
    (void)stop;
    if (![key isKindOfClass:[NSString class]]) {
      return;
    }
    NSString *stringKey = key;
    NSString *stringValue = nil;
    if ([obj isKindOfClass:[NSString class]]) {
      stringValue = obj;
    } else if ([obj respondsToSelector:@selector(stringValue)]) {
      stringValue = [obj stringValue];
    } else {
      stringValue = [obj description];
    }
    if (stringValue == nil) {
      stringValue = @"";
    }
    [keys addObject:stringKey];
    [values addObject:stringValue];
  }];

  size_t count = keys.count;
  struct NametagPushField *fields = NULL;
  if (count > 0) {
    fields = malloc(count * sizeof(struct NametagPushField));
  }
  NSMutableArray<NSData *> *keyData =
      [[NSMutableArray alloc] initWithCapacity:count];
  NSMutableArray<NSData *> *valueData =
      [[NSMutableArray alloc] initWithCapacity:count];
  for (size_t index = 0; index < count; index++) {
    NSData *k = [keys[index] dataUsingEncoding:NSUTF8StringEncoding];
    NSData *v = [values[index] dataUsingEncoding:NSUTF8StringEncoding];
    [keyData addObject:k];
    [valueData addObject:v];
    fields[index].key.ptr = (const uint8_t *)k.bytes;
    fields[index].key.len = k.length;
    fields[index].value.ptr = (const uint8_t *)v.bytes;
    fields[index].value.len = v.length;
  }

  NSData *soundData = nil;
  if (soundName.length > 0) {
    soundData = [soundName dataUsingEncoding:NSUTF8StringEncoding];
  }

  struct NametagPushNotification payload;
  payload.fields = fields;
  payload.field_count = count;
  payload.sound.ptr =
      soundData.length > 0 ? (const uint8_t *)soundData.bytes : NULL;
  payload.sound.len = soundData.length;
  payload.has_badge = badgeNumber != nil;
  payload.badge = badgeNumber != nil ? (int32_t)badgeNumber.integerValue : 0;
  g_push_callback(NAMETAG_PUSH_EVENT_NOTIFICATION, &payload, g_push_context);

  if (fields != NULL) {
    free(fields);
  }
#endif
}

static void dispatch_notification_response(NSDictionary *userInfo) {
#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
  if (nametag_ios_handle_notification_response != NULL) {
    nametag_ios_handle_notification_response(userInfo);
  }
#else
  (void)userInfo;
#endif
}

@interface OxidePushBridge : NSObject <UNUserNotificationCenterDelegate>
@property(nonatomic, strong) NSString *deviceToken;
@property(nonatomic, strong) UNUserNotificationCenter *center;
- (void)registerForPush;
- (void)refreshAuthorization;
@end

@implementation OxidePushBridge

- (instancetype)init {
  self = [super init];
  if (!self) {
    return nil;
  }
  _center = [UNUserNotificationCenter currentNotificationCenter];
  _center.delegate = self;
  [self refreshAuthorization];
  return self;
}

- (void)storeTokenData:(NSData *)data {
  if (data.length == 0) {
    self.deviceToken = nil;
    emit_token_event(@"");
    return;
  }
  NSString *token = string_from_data(data);
  self.deviceToken = token;
  emit_token_event(token);
}

- (void)registerForPush {
  if (suppress_push_prompt()) {
    publish_notification_permission(UNAuthorizationStatusAuthorized);
    return;
  }
  UNAuthorizationOptions options = UNAuthorizationOptionAlert |
                                   UNAuthorizationOptionSound |
                                   UNAuthorizationOptionBadge;
  [self.center requestAuthorizationWithOptions:options
                             completionHandler:^(BOOL granted,
                                                 NSError *error) {
                               (void)granted;
                               (void)error;
                               dispatch_async(dispatch_get_main_queue(), ^{
                                 [[UIApplication sharedApplication]
                                     registerForRemoteNotifications];
                               });
                               dispatch_async(push_queue(), ^{
                                 [self refreshAuthorization];
                               });
                             }];
}

- (void)refreshAuthorization {
  if (suppress_push_prompt()) {
    publish_notification_permission(UNAuthorizationStatusAuthorized);
    return;
  }
  [self.center getNotificationSettingsWithCompletionHandler:^(
                   UNNotificationSettings *settings) {
    UNAuthorizationStatus status = settings != nil
                                       ? settings.authorizationStatus
                                       : UNAuthorizationStatusNotDetermined;
    dispatch_async(push_queue(), ^{
      publish_notification_permission(status);
    });
  }];
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
    didReceiveNotificationResponse:(UNNotificationResponse *)response
             withCompletionHandler:(void (^)(void))completionHandler {
  NSDictionary *userInfo = response.notification.request.content.userInfo;
  emit_notification_event(userInfo);
  dispatch_notification_response(userInfo);
  if (completionHandler) {
    completionHandler();
  }
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       willPresentNotification:(UNNotification *)notification
         withCompletionHandler:
             (void (^)(UNNotificationPresentationOptions options))
                 completionHandler {
  emit_notification_event(notification.request.content.userInfo);
  if (completionHandler) {
    completionHandler(UNNotificationPresentationOptionList |
                      UNNotificationPresentationOptionBanner |
                      UNNotificationPresentationOptionSound |
                      UNNotificationPresentationOptionBadge);
  }
}

@end

static OxidePushBridge *push_bridge(void) {
  static OxidePushBridge *instance = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    instance = [[OxidePushBridge alloc] init];
  });
  return instance;
}

void oxide_host_push_bootstrap(void) { (void)push_bridge(); }

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_bootstrap(void) { oxide_host_push_bootstrap(); }
#endif

void oxide_host_push_register(void) {
  dispatch_async(push_queue(), ^{
    [push_bridge() registerForPush];
  });
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_register(void) { oxide_host_push_register(); }
#endif

void oxide_host_push_set_badge(int32_t count) {
  dispatch_async(dispatch_get_main_queue(), ^{
    if (@available(iOS 17.0, *)) {
      [[UNUserNotificationCenter currentNotificationCenter]
          setBadgeCount:(NSInteger)count
  withCompletionHandler:nil];
    } else {
      UIApplication *application = UIApplication.sharedApplication;
      SEL selector = NSSelectorFromString(@"setApplicationIconBadgeNumber:");
      if (application != nil && [application respondsToSelector:selector]) {
        NSMethodSignature *signature =
            [application methodSignatureForSelector:selector];
        if (signature != nil) {
          NSInvocation *invocation =
              [NSInvocation invocationWithMethodSignature:signature];
          NSInteger badge = (NSInteger)count;
          invocation.selector = selector;
          invocation.target = application;
          [invocation setArgument:&badge atIndex:2];
          [invocation invoke];
        }
      }
    }
  });
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_set_badge(int32_t count) {
  oxide_host_push_set_badge(count);
}
#endif

void oxide_host_push_clear_badge(void) { oxide_host_push_set_badge(0); }

void oxide_host_push_clear_all_delivered(void)
{
  [[UNUserNotificationCenter currentNotificationCenter]
      removeAllDeliveredNotifications];
  oxide_host_push_clear_badge();
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_clear_badge(void) { oxide_host_push_clear_badge(); }
#endif

int oxide_host_push_get_device_token(char **out_ptr, size_t *out_len) {
  if (out_ptr != NULL) {
    *out_ptr = NULL;
  }
  if (out_len != NULL) {
    *out_len = 0;
  }

  __block NSString *token = nil;
  dispatch_sync(push_queue(), ^{
    token = push_bridge().deviceToken;
  });
  if (token.length == 0) {
    return 0;
  }

  NSData *utf8 = [token dataUsingEncoding:NSUTF8StringEncoding];
  char *copy = malloc(utf8.length);
  if (copy == NULL) {
    return 0;
  }
  memcpy(copy, utf8.bytes, utf8.length);
  if (out_ptr != NULL) {
    *out_ptr = copy;
  } else {
    free(copy);
  }
  if (out_len != NULL) {
    *out_len = utf8.length;
  }
  return 1;
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
bool nametag_ios_push_device_token(struct NametagHostUtf8 *out) {
  __block NSString *token = nil;
  dispatch_sync(push_queue(), ^{
    token = push_bridge().deviceToken;
  });
  if (token.length == 0) {
    if (out != NULL) {
      out->ptr = NULL;
      out->len = 0;
    }
    return false;
  }
  NSData *utf8 = [token dataUsingEncoding:NSUTF8StringEncoding];
  uint8_t *copy = malloc(utf8.length);
  if (copy == NULL) {
    return false;
  }
  memcpy(copy, utf8.bytes, utf8.length);
  if (out != NULL) {
    out->ptr = copy;
    out->len = utf8.length;
  } else {
    free(copy);
  }
  return true;
}

void nametag_ios_push_free_utf8(struct NametagHostUtf8 buffer) {
  if (buffer.ptr != NULL) {
    free((void *)buffer.ptr);
  }
}

void nametag_ios_push_subscribe(NametagPushEventCallback cb, void *ctx) {
  dispatch_async(push_queue(), ^{
    g_push_callback = cb;
    g_push_context = ctx;
    if (push_bridge().deviceToken.length > 0) {
      emit_token_event(push_bridge().deviceToken);
    }
    [push_bridge() refreshAuthorization];
  });
}
#endif

void oxide_host_push_application_did_register(NSData *deviceToken) {
  dispatch_async(push_queue(), ^{
    [push_bridge() storeTokenData:deviceToken];
    [push_bridge() refreshAuthorization];
  });
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_application_did_register(NSData *deviceToken) {
  oxide_host_push_application_did_register(deviceToken);
}
#endif

void oxide_host_push_application_did_fail(NSError *error) {
  (void)error;
  dispatch_async(push_queue(), ^{
    [push_bridge() refreshAuthorization];
  });
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_application_did_fail(NSError *error) {
  oxide_host_push_application_did_fail(error);
}
#endif

void oxide_host_push_application_did_receive(NSDictionary *userInfo) {
  dispatch_async(push_queue(), ^{
    emit_notification_event(userInfo);
    [push_bridge() refreshAuthorization];
  });
}

#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE
void nametag_ios_push_application_did_receive(NSDictionary *userInfo) {
  oxide_host_push_application_did_receive(userInfo);
}
#endif
