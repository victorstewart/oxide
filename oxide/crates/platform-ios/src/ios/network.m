#import <Foundation/Foundation.h>
#import <Network/Network.h>
#import <Security/Security.h>
#import <Security/SecProtocolMetadata.h>
#import <Security/SecProtocolOptions.h>
#import <Security/SecProtocolTypes.h>
#import <dispatch/dispatch.h>
#import <math.h>
#import <stdbool.h>
#import <stdint.h>

struct NametagQuicConfig
{
   uint32_t idle_timeout_ms;
   uint16_t max_datagram_size;
   bool allow_fallback;
   const char *alpn;
   bool force_tcp_tls;
};

struct NametagRetryPolicy
{
   uint32_t max_attempts;
   uint64_t initial_backoff_ms;
   uint64_t max_backoff_ms;
};

struct NametagTrustAnchor
{
   const uint8_t *data;
   size_t len;
};

struct NametagQuicTlsConfig
{
   const uint8_t *identity_der;
   size_t identity_der_len;
   const uint8_t *private_key_pkcs8;
   size_t private_key_pkcs8_len;
   const struct NametagTrustAnchor *trust_anchors;
   size_t trust_anchor_count;
   bool enforce_hostname;
};

struct NametagQuicMetrics
{
   uint64_t handshake_ms;
   uint64_t resume_ms;
   uint64_t payload_bytes;
   uint64_t total_bytes;
   uint32_t attempts;
   bool fallback_used;
};

struct NametagReachabilityStatus
{
   bool reachable;
   bool expensive;
   uint8_t path_kind;
};

typedef void *NametagQuicHandle;
typedef void *NametagReachabilityHandle;

struct OxideHttpResponse
{
   uint16_t status;
   uint8_t *body_ptr;
   size_t body_len;
   uint8_t *final_url_ptr;
   size_t final_url_len;
   uint8_t *content_type_ptr;
   size_t content_type_len;
};

void oxide_host_http_response_free(struct OxideHttpResponse *response);

static const uint32_t kNametagDefaultPort = 443;
static void (*g_oxide_reachability_callback)(uint32_t status, uint32_t iface,
                                             uint8_t expensive) = NULL;
static id g_oxide_reachability_monitor = nil;

static dispatch_queue_t quic_queue(void)
{
   static dispatch_queue_t queue;
   static dispatch_once_t once;
   dispatch_once(&once, ^{
      queue =
          dispatch_queue_create("com.oxide.platform.network.quic",
                                DISPATCH_QUEUE_SERIAL);
   });
   return queue;
}

static bool copy_http_bytes(NSData *data, uint8_t **out_ptr, size_t *out_len)
{
   if (out_ptr == NULL || out_len == NULL)
   {
      return false;
   }
   *out_ptr = NULL;
   *out_len = 0;
   if (data == nil || data.length == 0)
   {
      return true;
   }
   uint8_t *buffer = (uint8_t *)malloc(data.length);
   if (buffer == NULL)
   {
      return false;
   }
   memcpy(buffer, data.bytes, data.length);
   *out_ptr = buffer;
   *out_len = data.length;
   return true;
}

static bool copy_http_string(NSString *string,
                             uint8_t **out_ptr,
                             size_t *out_len)
{
   NSData *data = string == nil ? nil : [string dataUsingEncoding:NSUTF8StringEncoding];
   return copy_http_bytes(data, out_ptr, out_len);
}

static BOOL env_truthy(const char *name)
{
   const char *value = getenv(name);
   if (value == NULL)
   {
      return NO;
   }
   return strcasecmp(value, "1") == 0 || strcasecmp(value, "true") == 0 ||
          strcasecmp(value, "yes") == 0 || strcasecmp(value, "on") == 0;
}

static void clear_http_response(struct OxideHttpResponse *response)
{
   if (response != NULL)
   {
      memset(response, 0, sizeof(*response));
   }
}

static uint8_t path_kind_for_path(nw_path_t path)
{
   __block uint8_t kind = 3; // Unknown
   if (path == NULL)
   {
      return kind;
   }

   nw_path_enumerate_interfaces(path, ^bool(nw_interface_t interface) {
     if (interface == NULL)
     {
        return true;
     }
     nw_interface_type_t type = nw_interface_get_type(interface);
     if (type == nw_interface_type_wifi)
     {
        kind = 0;
        return false;
     }
     if (type == nw_interface_type_cellular)
     {
        kind = 1;
        return false;
     }
#if defined(nw_interface_type_wiredEthernet)
     if (type == nw_interface_type_wiredEthernet)
     {
        kind = 2;
        return false;
     }
#endif
#if defined(nw_interface_type_wired)
     if (type == nw_interface_type_wired)
     {
        kind = 2;
        return false;
     }
#endif
     return true;
   });
   return kind;
}

static NSArray *copy_trust_anchors(const struct NametagQuicTlsConfig *tls)
{
   if (tls == NULL || tls->trust_anchors == NULL ||
       tls->trust_anchor_count == 0)
   {
      return nil;
   }
   NSMutableArray *anchors =
       [[NSMutableArray alloc] initWithCapacity:tls->trust_anchor_count];
   for (size_t index = 0; index < tls->trust_anchor_count; index++)
   {
      const struct NametagTrustAnchor anchor = tls->trust_anchors[index];
      CFDataRef data =
          CFDataCreate(kCFAllocatorDefault, anchor.data, (CFIndex)anchor.len);
      if (data == NULL)
      {
         continue;
      }
      SecCertificateRef certificate =
          SecCertificateCreateWithData(kCFAllocatorDefault, data);
      CFRelease(data);
      if (certificate != NULL)
      {
         [anchors addObject:(__bridge_transfer id)certificate];
      }
   }
   return anchors.count > 0 ? anchors : nil;
}

static SecIdentityRef copy_identity(const struct NametagQuicTlsConfig *tls)
{
   if (tls == NULL || tls->identity_der == NULL || tls->identity_der_len == 0
       || tls->private_key_pkcs8 == NULL ||
       tls->private_key_pkcs8_len == 0)
   {
      return NULL;
   }

   CFDataRef cert_data =
       CFDataCreate(NULL, tls->identity_der, (CFIndex)tls->identity_der_len);
   if (cert_data == NULL)
   {
      return NULL;
   }
   SecCertificateRef certificate = SecCertificateCreateWithData(NULL, cert_data);
   CFRelease(cert_data);
   if (certificate == NULL)
   {
      return NULL;
   }

   CFDataRef key_data = CFDataCreate(NULL, tls->private_key_pkcs8,
                                     (CFIndex)tls->private_key_pkcs8_len);
   if (key_data == NULL)
   {
      CFRelease(certificate);
      return NULL;
   }

   CFDictionaryRef attributes = (__bridge CFDictionaryRef)@{
      (__bridge NSString *)kSecAttrKeyType :
          (__bridge NSString *)kSecAttrKeyTypeRSA,
      (__bridge NSString *)kSecAttrKeyClass :
          (__bridge NSString *)kSecAttrKeyClassPrivate,
      (__bridge NSString *)kSecAttrKeySizeInBits : @(2048),
   };
   SecKeyRef private_key = SecKeyCreateWithData(key_data, attributes, NULL);
   CFRelease(key_data);
   if (private_key == NULL)
   {
      CFRelease(certificate);
      return NULL;
   }

   SecIdentityRef identity =
       SecIdentityCreate(NULL, certificate, private_key);
   CFRelease(certificate);
   CFRelease(private_key);
   return identity;
}

static void configure_sec_options(sec_protocol_options_t sec_options,
                                  const struct NametagQuicTlsConfig *tls,
                                  const char *server_name,
                                  NSString *alpn)
{
   if (sec_options == NULL)
   {
      return;
   }

   if (server_name != NULL && server_name[0] != '\0')
   {
      sec_protocol_options_set_tls_server_name(sec_options, server_name);
   }

   if (alpn != nil && alpn.length > 0)
   {
      sec_protocol_options_add_tls_application_protocol(sec_options, alpn.UTF8String);
   }

   if (tls != NULL && !tls->enforce_hostname)
   {
      sec_protocol_options_set_peer_authentication_required(sec_options,
                                                            false);
   }

   SecIdentityRef identity_ref = copy_identity(tls);
   if (identity_ref != NULL)
   {
      sec_identity_t identity = sec_identity_create(identity_ref);
      CFRelease(identity_ref);
      if (identity != NULL)
      {
         sec_protocol_options_set_local_identity(sec_options, identity);
      }
   }

   NSArray *anchors = copy_trust_anchors(tls);
   if (anchors.count > 0)
   {
      NSArray *anchors_copy = [anchors copy];
      sec_protocol_options_set_verify_block(
          sec_options,
          ^(sec_protocol_metadata_t metadata, sec_trust_t trust_ref,
            sec_protocol_verify_complete_t complete) {
            (void)metadata;
            BOOL ok = NO;
            if (trust_ref != NULL)
            {
               SecTrustRef trust = sec_trust_copy_ref(trust_ref);
               if (trust != NULL)
               {
                  CFArrayRef anchor_array = (__bridge CFArrayRef)anchors_copy;
                  OSStatus status =
                      SecTrustSetAnchorCertificates(trust, anchor_array);
                  if (status == errSecSuccess)
                  {
                     status =
                         SecTrustSetAnchorCertificatesOnly(trust, true);
                     if (status == errSecSuccess &&
                         SecTrustEvaluateWithError(trust, NULL))
                     {
                        ok = YES;
                     }
                  }
                  CFRelease(trust);
               }
            }
            complete(ok);
          },
          dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0));
   }
}

@interface NametagQuicConnection : NSObject
{
   nw_connection_t _connection;
   nw_parameters_t _quicParameters;
   nw_parameters_t _tlsParameters;
}

@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic, assign) struct NametagQuicConfig quicConfig;
@property(nonatomic, assign) struct NametagRetryPolicy retryPolicy;
@property(nonatomic, assign) struct NametagQuicMetrics metrics;
@property(nonatomic, assign) BOOL fallbackUsed;
@property(nonatomic, assign) BOOL ready;
@property(nonatomic, strong) dispatch_semaphore_t readySignal;
@property(nonatomic, strong) dispatch_semaphore_t receiveSignal;
@property(nonatomic, assign) NSUInteger attempt;
@property(nonatomic, assign) BOOL closed;
@property(nonatomic, copy) NSString *host;
@property(nonatomic, assign) uint16_t port;
@property(nonatomic, strong) NSDate *handshakeStart;
@property(nonatomic, strong) NSMutableArray<NSData *> *receiveBuffer;

- (instancetype)initWithEndpoint:(NSString *)endpoint
                            port:(uint16_t)port
                             cfg:(const struct NametagQuicConfig *)cfg
                           retry:(const struct NametagRetryPolicy *)retry
                             tls:(const struct NametagQuicTlsConfig *)tls;
- (void)start;
- (void)close;
- (BOOL)copyMetrics:(struct NametagQuicMetrics *)outMetrics;

@end

@interface NametagQuicConnection ()

- (void)startAttemptWithParameters:(nw_parameters_t)parameters
                          fallback:(BOOL)fallback;
- (void)handleReady;
- (void)handleFailure:(nw_error_t)error fallback:(BOOL)attemptedFallback;
- (void)scheduleRetryWithParameters:(nw_parameters_t)parameters
                            fallback:(BOOL)fallback;
- (void)startReceiveLoop;
- (BOOL)waitForReady:(uint64_t)timeoutMs;
- (BOOL)sendBytes:(const uint8_t *)data
            length:(size_t)len
           timeout:(uint64_t)timeoutMs;
- (NSData *)popReceived:(uint64_t)timeoutMs;

@end

@implementation NametagQuicConnection

- (instancetype)initWithEndpoint:(NSString *)endpoint
                            port:(uint16_t)port
                             cfg:(const struct NametagQuicConfig *)cfg
                           retry:(const struct NametagRetryPolicy *)retry
                             tls:(const struct NametagQuicTlsConfig *)tls
{
   self = [super init];
   if (!self)
   {
      return nil;
   }
   _queue = quic_queue();
   _host = [endpoint copy];
   _port = port;
   _quicConfig = *cfg;
   _retryPolicy = *retry;
   NSString *alpn = nil;
   if (cfg->alpn != NULL && cfg->alpn[0] != '\0')
   {
      alpn = [NSString stringWithUTF8String:cfg->alpn];
   }
   _metrics = (struct NametagQuicMetrics){0};
   _receiveBuffer = [[NSMutableArray alloc] init];
   _ready = NO;
   _readySignal = dispatch_semaphore_create(0);
   _receiveSignal = dispatch_semaphore_create(0);

   _quicParameters = nw_parameters_create_quic(^(
       nw_protocol_options_t quic_options) {
     sec_protocol_options_t sec_options =
         nw_quic_copy_sec_protocol_options(quic_options);
     configure_sec_options(sec_options, tls, endpoint.UTF8String, alpn);
   });
   if (_quicParameters == NULL)
   {
      return nil;
   }
   nw_parameters_set_prefer_no_proxy(_quicParameters, true);
   nw_parameters_set_reuse_local_address(_quicParameters, true);

   _tlsParameters = nw_parameters_create_secure_tcp(^(
       nw_protocol_options_t tls_options) {
     sec_protocol_options_t sec_options =
         nw_tls_copy_sec_protocol_options(tls_options);
     configure_sec_options(sec_options, tls, endpoint.UTF8String, alpn);
   }, ^(nw_protocol_options_t tcp_options) {
     (void)tcp_options;
   });
   if (_tlsParameters != NULL)
   {
      nw_parameters_set_prefer_no_proxy(_tlsParameters, true);
      nw_parameters_set_reuse_local_address(_tlsParameters, true);
   }

   return self;
}

- (void)dealloc
{
   [self close];
   _quicParameters = NULL;
   _tlsParameters = NULL;
}

- (void)start
{
   dispatch_sync(self.queue, ^{
     if (self.closed)
     {
        return;
     }
     if ((self.quicConfig.force_tcp_tls ||
          env_truthy("NAMETAG_NETWORK_FORCE_TCP_TLS")) &&
         self->_tlsParameters != NULL)
     {
        [self startAttemptWithParameters:self->_tlsParameters fallback:YES];
        return;
     }
     [self startAttemptWithParameters:self->_quicParameters fallback:NO];
   });
}

- (void)startAttemptWithParameters:(nw_parameters_t)parameters
                          fallback:(BOOL)fallback
{
   if (parameters == NULL || self.closed)
   {
      return;
   }

   self.ready = NO;
   self.readySignal = dispatch_semaphore_create(0);
   self.receiveSignal = dispatch_semaphore_create(0);
   [self.receiveBuffer removeAllObjects];

   self.handshakeStart = [NSDate date];
   self.attempt += 1;
   _metrics.attempts = (uint32_t)self.attempt;
   if (fallback)
   {
      self.fallbackUsed = YES;
      _metrics.fallback_used = true;
   }

   if (_connection != NULL)
   {
      nw_connection_cancel(_connection);
      _connection = NULL;
   }

   const char *hostCString = self.host.UTF8String;
   char portCString[8] = {0};
   snprintf(portCString, sizeof(portCString), "%u", self.port);
   nw_endpoint_t endpoint = nw_endpoint_create_host(hostCString, portCString);
   if (endpoint == NULL)
   {
      return;
   }

   nw_connection_t connection = nw_connection_create(endpoint, parameters);
   if (connection == NULL)
   {
      return;
   }
   _connection = connection;
   nw_connection_set_queue(_connection, self.queue);

   __weak typeof(self) weakSelf = self;
   nw_connection_set_state_changed_handler(
       _connection, ^(nw_connection_state_t state, nw_error_t error) {
         __strong typeof(self) strongSelf = weakSelf;
         if (!strongSelf)
         {
            return;
         }
         switch (state)
         {
         case nw_connection_state_ready:
            [strongSelf handleReady];
            break;
         case nw_connection_state_failed:
         case nw_connection_state_cancelled:
         case nw_connection_state_waiting:
            [strongSelf handleFailure:error fallback:fallback];
            break;
         default:
            break;
         }
       });

   nw_connection_start(_connection);
}

- (void)handleReady
{
   if (self.closed)
   {
      return;
   }

   NSTimeInterval elapsed =
       [[NSDate date] timeIntervalSinceDate:self.handshakeStart];
   _metrics.handshake_ms = (uint64_t)(elapsed * 1000.0);
   _metrics.resume_ms = 0;
   self.ready = YES;
   dispatch_semaphore_signal(self.readySignal);
   [self startReceiveLoop];
}

- (void)handleFailure:(nw_error_t)error fallback:(BOOL)attemptedFallback
{
   if (self.closed)
   {
      return;
   }

   BOOL canRetry = self.attempt < MAX(self.retryPolicy.max_attempts, 1);
   BOOL forceTcpTls =
       self.quicConfig.force_tcp_tls ||
       env_truthy("NAMETAG_NETWORK_FORCE_TCP_TLS");
   if (forceTcpTls && _tlsParameters != NULL && canRetry)
   {
      [self scheduleRetryWithParameters:_tlsParameters fallback:YES];
      return;
   }

   if (!attemptedFallback && self.quicConfig.allow_fallback &&
       _tlsParameters != NULL)
   {
      [self scheduleRetryWithParameters:_tlsParameters fallback:YES];
      return;
   }

   if (canRetry)
   {
      [self scheduleRetryWithParameters:_quicParameters fallback:NO];
      return;
   }

   if (error != NULL)
   {
      NSLog(@"Nametag QUIC connection failed: %@", error);
   }
}

- (void)scheduleRetryWithParameters:(nw_parameters_t)parameters
                            fallback:(BOOL)fallback
{
   if (parameters == NULL || self.closed)
   {
      return;
   }

   uint64_t exponent = (uint64_t)pow(2.0, (double)(self.attempt - 1));
   uint64_t delay_ms = self.retryPolicy.initial_backoff_ms * exponent;
   if (delay_ms > self.retryPolicy.max_backoff_ms)
   {
      delay_ms = self.retryPolicy.max_backoff_ms;
   }
   dispatch_time_t deadline = dispatch_time(
       DISPATCH_TIME_NOW, (int64_t)(delay_ms * NSEC_PER_MSEC));
   __weak typeof(self) weakSelf = self;
   dispatch_after(deadline, self.queue, ^{
     __strong typeof(self) strongSelf = weakSelf;
     if (!strongSelf || strongSelf.closed)
     {
        return;
     }
     [strongSelf startAttemptWithParameters:parameters fallback:fallback];
   });
}

- (BOOL)waitForReady:(uint64_t)timeoutMs
{
   if (self.closed)
   {
      return NO;
   }
   if (self.ready)
   {
      return YES;
   }
   dispatch_time_t deadline = dispatch_time(
       DISPATCH_TIME_NOW, (int64_t)(timeoutMs * NSEC_PER_MSEC));
   long result = dispatch_semaphore_wait(self.readySignal, deadline);
   return result == 0 && self.ready;
}

- (BOOL)sendBytes:(const uint8_t *)data
            length:(size_t)len
           timeout:(uint64_t)timeoutMs
{
   if (data == NULL || len == 0 || self.closed)
   {
      return NO;
   }
   if (![self waitForReady:timeoutMs])
   {
      return NO;
   }

   __block BOOL ok = NO;
   dispatch_semaphore_t sem = dispatch_semaphore_create(0);
   dispatch_data_t content =
       dispatch_data_create(data, len, NULL, DISPATCH_DATA_DESTRUCTOR_DEFAULT);
   nw_connection_send(_connection, content, NW_CONNECTION_DEFAULT_MESSAGE_CONTEXT,
                      true, ^(nw_error_t error) {
                        if (error == NULL)
                        {
                           ok = YES;
                        }
                        dispatch_semaphore_signal(sem);
                      });

   dispatch_time_t deadline = dispatch_time(
       DISPATCH_TIME_NOW, (int64_t)(timeoutMs * NSEC_PER_MSEC));
   dispatch_semaphore_wait(sem, deadline);
   return ok;
}

- (NSData *)popReceived:(uint64_t)timeoutMs
{
   __block NSData *payload = nil;
   __weak typeof(self) weakSelf = self;
   dispatch_sync(self.queue, ^{
     __strong typeof(self) strongSelf = weakSelf;
     if (strongSelf && strongSelf.receiveBuffer.count > 0)
     {
        payload = strongSelf.receiveBuffer.firstObject;
        [strongSelf.receiveBuffer removeObjectAtIndex:0];
     }
   });

   if (payload != nil || timeoutMs == 0)
   {
      return payload;
   }

   dispatch_time_t deadline = dispatch_time(
       DISPATCH_TIME_NOW, (int64_t)(timeoutMs * NSEC_PER_MSEC));
   if (dispatch_semaphore_wait(self.receiveSignal, deadline) != 0)
   {
      return nil;
   }

   __block NSData *after = nil;
   dispatch_sync(self.queue, ^{
     __strong typeof(self) strongSelf = weakSelf;
     if (strongSelf && strongSelf.receiveBuffer.count > 0)
     {
        after = strongSelf.receiveBuffer.firstObject;
        [strongSelf.receiveBuffer removeObjectAtIndex:0];
     }
   });
   return after;
}

- (void)startReceiveLoop
{
   if (self.closed || _connection == NULL)
   {
      return;
   }
   __weak typeof(self) weakSelf = self;
   nw_connection_receive_message(
       _connection,
       ^(dispatch_data_t content, nw_content_context_t context, bool isComplete,
         nw_error_t receiveError) {
         (void)context;
         (void)isComplete;
         __strong typeof(self) strongSelf = weakSelf;
         if (!strongSelf || strongSelf.closed)
         {
            return;
         }

         NSMutableData *payload = [[NSMutableData alloc] init];
         if (content != NULL)
         {
            dispatch_data_apply(
                content, ^bool(dispatch_data_t region, size_t offset,
                               const void *buffer, size_t size) {
                  (void)region;
                  (void)offset;
                  if (buffer != NULL && size > 0)
                  {
                     [payload appendBytes:buffer length:size];
                  }
                  return true;
                });
         }

         if (payload.length > 0)
         {
            strongSelf->_metrics.payload_bytes += payload.length;
            strongSelf->_metrics.total_bytes += payload.length;
            [strongSelf.receiveBuffer addObject:payload];
            dispatch_semaphore_signal(strongSelf.receiveSignal);
         }

         if (receiveError != NULL)
         {
            NSLog(@"Nametag QUIC receive error: %@", receiveError);
         }
         [strongSelf startReceiveLoop];
       });
}

- (BOOL)copyMetrics:(struct NametagQuicMetrics *)outMetrics
{
   if (!outMetrics)
   {
      return false;
   }

   __block BOOL ok = NO;
   dispatch_sync(self.queue, ^{
     *outMetrics = self->_metrics;
     ok = YES;
   });
   return ok;
}

- (void)close
{
   if (self.closed)
   {
      return;
   }
   self.closed = YES;
   if (_connection != NULL)
   {
      nw_connection_cancel(_connection);
      _connection = NULL;
   }
}

@end

@interface NametagReachabilityMonitor : NSObject
{
   nw_path_monitor_t _monitor;
}

@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic, assign) BOOL reachable;
@property(nonatomic, assign) BOOL expensive;
@property(nonatomic, assign) uint8_t pathKind;

- (instancetype)init;
- (void)start;
- (void)stop;
- (BOOL)copyStatus:(struct NametagReachabilityStatus *)outStatus;

@end

static void emit_oxide_reachability_snapshot(BOOL reachable, uint8_t pathKind,
                                             BOOL expensive)
{
   if (g_oxide_reachability_callback != NULL)
   {
      g_oxide_reachability_callback(reachable ? 1u : 0u, pathKind,
                                    expensive ? 1u : 0u);
   }
}

@implementation NametagReachabilityMonitor

- (instancetype)init
{
   self = [super init];
   if (!self)
   {
      return nil;
   }
   _monitor = NULL;
   _reachable = NO;
   _expensive = NO;
   _pathKind = 3;
   _queue = dispatch_queue_create("com.oxide.platform.network.reachability",
                                  DISPATCH_QUEUE_SERIAL);
   return self;
}

- (void)dealloc
{
   [self stop];
}

- (void)start
{
   if (_monitor != NULL)
   {
      return;
   }
   _monitor = nw_path_monitor_create();
   if (_monitor == NULL)
   {
      return;
   }
   nw_path_monitor_set_queue(_monitor, self.queue);
   __weak typeof(self) weakSelf = self;
   nw_path_monitor_set_update_handler(_monitor, ^(nw_path_t path) {
     __strong typeof(self) strongSelf = weakSelf;
     if (!strongSelf)
     {
        return;
     }
     strongSelf.reachable =
         nw_path_get_status(path) == nw_path_status_satisfied;
     strongSelf.expensive = nw_path_is_expensive(path);
     strongSelf.pathKind = path_kind_for_path(path);
     emit_oxide_reachability_snapshot(strongSelf.reachable, strongSelf.pathKind,
                                      strongSelf.expensive);
   });
   nw_path_monitor_start(_monitor);
}

- (void)stop
{
   if (_monitor != NULL)
   {
      nw_path_monitor_cancel(_monitor);
      _monitor = NULL;
   }
}

- (BOOL)copyStatus:(struct NametagReachabilityStatus *)outStatus
{
   if (outStatus == NULL)
   {
      return NO;
   }
   __block BOOL ok = NO;
   __weak typeof(self) weakSelf = self;
   dispatch_sync(self.queue, ^{
     __strong typeof(self) strongSelf = weakSelf;
     if (!strongSelf)
     {
        return;
     }
     outStatus->reachable = strongSelf.reachable;
     outStatus->expensive = strongSelf.expensive;
     outStatus->path_kind = strongSelf.pathKind;
     ok = YES;
   });
   return ok;
}

@end

static BOOL parse_endpoint(const char *endpoint, NSString **host,
                           uint16_t *port)
{
   if (endpoint == NULL)
   {
      return NO;
   }
   NSString *raw = [NSString stringWithUTF8String:endpoint];
   if (raw.length == 0)
   {
      return NO;
   }

   NSString *mutable = [raw
       stringByTrimmingCharactersInSet:
           [NSCharacterSet whitespaceAndNewlineCharacterSet]];
   if (mutable.length == 0)
   {
      return NO;
   }

   NSString *hostPart = nil;
   NSString *portPart = nil;
   if ([mutable hasPrefix:@"["])
   {
      NSRange closing = [mutable rangeOfString:@"]"
                                        options:NSBackwardsSearch];
      if (closing.location == NSNotFound)
      {
         return NO;
      }
      hostPart = [mutable
          substringWithRange:NSMakeRange(1, closing.location - 1)];
      if (closing.location + 1 < mutable.length &&
          [mutable characterAtIndex:closing.location + 1] == ':')
      {
         portPart = [mutable substringFromIndex:closing.location + 2];
      }
   }
   else
   {
      NSRange colon = [mutable rangeOfString:@":"
                                     options:NSBackwardsSearch];
      if (colon.location != NSNotFound)
      {
         hostPart = [mutable substringToIndex:colon.location];
         portPart = [mutable substringFromIndex:colon.location + 1];
      }
      else
      {
         hostPart = mutable;
      }
   }

   if (hostPart.length == 0)
   {
      return NO;
   }
   uint16_t resolvedPort = kNametagDefaultPort;
   if (portPart.length > 0)
   {
      NSInteger parsed = portPart.integerValue;
      if (parsed <= 0 || parsed > UINT16_MAX)
      {
         return NO;
      }
      resolvedPort = (uint16_t)parsed;
   }

   *host = hostPart;
   *port = resolvedPort;
   return YES;
}

NametagQuicHandle nametag_ios_quic_connect(
    const char *endpoint,
    const struct NametagQuicConfig *cfg,
    const struct NametagRetryPolicy *retry,
    const struct NametagQuicTlsConfig *tls)
{
   NSString *host = nil;
   uint16_t port = kNametagDefaultPort;
   if (!parse_endpoint(endpoint, &host, &port))
   {
      return NULL;
   }

   struct NametagQuicConfig localCfg =
       cfg ? *cfg : (struct NametagQuicConfig){60000, 1350, true, NULL, false};
   struct NametagRetryPolicy localRetry =
       retry ? *retry : (struct NametagRetryPolicy){3, 500, 8000};

   NametagQuicConnection *connection = [[NametagQuicConnection alloc]
       initWithEndpoint:host
                   port:port
                    cfg:&localCfg
                  retry:&localRetry
                    tls:tls];
   if (!connection)
   {
      return NULL;
   }
   [connection start];
   return (void *)CFBridgingRetain(connection);
}

bool nametag_ios_quic_metrics(NametagQuicHandle handle,
                              struct NametagQuicMetrics *outMetrics)
{
   if (handle == NULL || outMetrics == NULL)
   {
      return false;
   }
   NametagQuicConnection *connection =
       (__bridge NametagQuicConnection *)handle;
   return [connection copyMetrics:outMetrics];
}

bool nametag_ios_quic_wait_ready(NametagQuicHandle handle,
                                 uint64_t timeout_ms)
{
   if (handle == NULL)
   {
      return false;
   }
   NametagQuicConnection *connection =
       (__bridge NametagQuicConnection *)handle;
   return [connection waitForReady:timeout_ms];
}

void nametag_ios_quic_close(NametagQuicHandle handle)
{
   if (handle == NULL)
   {
      return;
   }
   NametagQuicConnection *connection =
       (__bridge_transfer NametagQuicConnection *)handle;
   [connection close];
}

bool nametag_ios_quic_send(NametagQuicHandle handle,
                           const uint8_t *data,
                           size_t len,
                           uint64_t timeout_ms)
{
   if (handle == NULL || data == NULL || len == 0)
   {
      return false;
   }
   NametagQuicConnection *connection =
       (__bridge NametagQuicConnection *)handle;
   return [connection sendBytes:data length:len timeout:timeout_ms];
}

bool nametag_ios_quic_recv(NametagQuicHandle handle,
                           uint8_t *buffer,
                           size_t buffer_len,
                           size_t *out_len,
                           uint64_t timeout_ms)
{
   if (handle == NULL || buffer == NULL || out_len == NULL)
   {
      return false;
   }

   NametagQuicConnection *connection =
       (__bridge NametagQuicConnection *)handle;
   NSData *payload = [connection popReceived:timeout_ms];
   if (payload == nil || payload.length == 0)
   {
      return false;
   }

   if (payload.length > buffer_len)
   {
      return false;
   }
   memcpy(buffer, payload.bytes, payload.length);
   *out_len = payload.length;
   return true;
}

int32_t oxide_host_http_get(const uint8_t *url_ptr,
                            size_t url_len,
                            uint32_t timeout_ms,
                            size_t max_response_bytes,
                            struct OxideHttpResponse *out_response)
{
   if (out_response == NULL)
   {
      return -1;
   }
   clear_http_response(out_response);
   if (url_ptr == NULL || url_len == 0 || max_response_bytes == 0)
   {
      return -1;
   }
   if ([NSThread isMainThread])
   {
      return -6;
   }

   NSString *url_string =
       [[NSString alloc] initWithBytes:url_ptr
                                length:url_len
                              encoding:NSUTF8StringEncoding];
   NSURL *url = url_string == nil ? nil : [NSURL URLWithString:url_string];
   NSString *scheme = url.scheme.lowercaseString;
   if (url == nil ||
       !([scheme isEqualToString:@"https"] || [scheme isEqualToString:@"http"]))
   {
      return -1;
   }

   NSTimeInterval timeout =
       timeout_ms == 0 ? 10.0 : ((NSTimeInterval)timeout_ms / 1000.0);
   NSURLSessionConfiguration *configuration =
       [NSURLSessionConfiguration ephemeralSessionConfiguration];
   configuration.timeoutIntervalForRequest = timeout;
   configuration.timeoutIntervalForResource = timeout;
   configuration.waitsForConnectivity = YES;
   configuration.requestCachePolicy = NSURLRequestReloadIgnoringLocalCacheData;
   configuration.URLCache = nil;
   configuration.allowsCellularAccess = YES;
#if TARGET_OS_IPHONE
   configuration.multipathServiceType = NSURLSessionMultipathServiceTypeHandover;
#endif

   NSMutableURLRequest *request =
       [NSMutableURLRequest requestWithURL:url
                               cachePolicy:NSURLRequestReloadIgnoringLocalCacheData
                           timeoutInterval:timeout];
   request.HTTPMethod = @"GET";

   dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
   __block NSData *body_data = nil;
   __block NSURLResponse *url_response = nil;
   __block NSError *request_error = nil;

   NSURLSession *session =
       [NSURLSession sessionWithConfiguration:configuration];
   NSURLSessionDataTask *task =
       [session dataTaskWithRequest:request
                  completionHandler:^(NSData *data,
                                      NSURLResponse *response,
                                      NSError *error) {
                    body_data = data;
                    url_response = response;
                    request_error = error;
                    dispatch_semaphore_signal(semaphore);
                  }];
   [task resume];

   int64_t wait_ns = (int64_t)((timeout + 5.0) * (NSTimeInterval)NSEC_PER_SEC);
   if (dispatch_semaphore_wait(
           semaphore, dispatch_time(DISPATCH_TIME_NOW, wait_ns)) != 0)
   {
      [task cancel];
      [session invalidateAndCancel];
      return -2;
   }
   [session finishTasksAndInvalidate];

   if (request_error != nil)
   {
      return -2;
   }
   if (![url_response isKindOfClass:[NSHTTPURLResponse class]])
   {
      return -3;
   }

   NSHTTPURLResponse *http_response = (NSHTTPURLResponse *)url_response;
   if (body_data.length > max_response_bytes)
   {
      return -4;
   }

   id raw_content_type = http_response.allHeaderFields[@"Content-Type"];
   NSString *content_type = [raw_content_type isKindOfClass:[NSString class]]
                                 ? (NSString *)raw_content_type
                                 : http_response.MIMEType;

   out_response->status = (uint16_t)http_response.statusCode;
   if (!copy_http_bytes(body_data, &out_response->body_ptr,
                        &out_response->body_len) ||
       !copy_http_string(http_response.URL.absoluteString,
                         &out_response->final_url_ptr,
                         &out_response->final_url_len) ||
       !copy_http_string(content_type, &out_response->content_type_ptr,
                         &out_response->content_type_len))
   {
      oxide_host_http_response_free(out_response);
      return -5;
   }
   return 0;
}

void oxide_host_http_response_free(struct OxideHttpResponse *response)
{
   if (response == NULL)
   {
      return;
   }
   free(response->body_ptr);
   free(response->final_url_ptr);
   free(response->content_type_ptr);
   clear_http_response(response);
}

NametagReachabilityHandle nametag_ios_reachability_start(void)
{
   NametagReachabilityMonitor *monitor =
       [[NametagReachabilityMonitor alloc] init];
   if (!monitor)
   {
      return NULL;
   }
   [monitor start];
   return (void *)CFBridgingRetain(monitor);
}

bool nametag_ios_reachability_poll(
    NametagReachabilityHandle handle,
    struct NametagReachabilityStatus *outStatus)
{
   if (handle == NULL || outStatus == NULL)
   {
      return false;
   }
   NametagReachabilityMonitor *monitor =
       (__bridge NametagReachabilityMonitor *)handle;
   return [monitor copyStatus:outStatus];
}

void nametag_ios_reachability_close(NametagReachabilityHandle handle)
{
   if (handle == NULL)
   {
      return;
   }
   NametagReachabilityMonitor *monitor =
       (__bridge_transfer NametagReachabilityMonitor *)handle;
   [monitor stop];
}

void oxide_host_net_set_reachability_callback(void (*cb)(uint32_t status,
                                                         uint32_t iface,
                                                         uint8_t expensive))
{
   g_oxide_reachability_callback = cb;
}

int32_t oxide_host_net_start_reachability(void)
{
   if (g_oxide_reachability_monitor == nil)
   {
      g_oxide_reachability_monitor = [[NametagReachabilityMonitor alloc] init];
      if (g_oxide_reachability_monitor == nil)
      {
         return -1;
      }
      [(NametagReachabilityMonitor *)g_oxide_reachability_monitor start];
   }

   struct NametagReachabilityStatus snapshot;
   memset(&snapshot, 0, sizeof(snapshot));
   if ([(NametagReachabilityMonitor *)g_oxide_reachability_monitor
           copyStatus:&snapshot])
   {
      emit_oxide_reachability_snapshot(snapshot.reachable, snapshot.path_kind,
                                       snapshot.expensive);
   }
   return 0;
}

void oxide_host_net_stop_reachability(void)
{
   if (g_oxide_reachability_monitor != nil)
   {
      [(NametagReachabilityMonitor *)g_oxide_reachability_monitor stop];
      g_oxide_reachability_monitor = nil;
   }
}
