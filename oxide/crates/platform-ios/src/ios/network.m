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
#import <time.h>

#import "network.h"

static const size_t kOxideMaxFrameBytes = 16 * 1024 * 1024;
static const NSUInteger kOxideMaxQueuedReceiveFrames = 64;
static const NSUInteger kOxideMaxQueuedReceiveBytes = 32 * 1024 * 1024;
static char kOxideQuicQueueContext;
static void (*g_oxide_reachability_callback)(uint32_t status, uint32_t iface,
                                             uint8_t expensive) = NULL;
static id g_oxide_reachability_monitor = nil;

enum OxideSendOutcome
{
   OxideSendOutcomePending = 0,
   OxideSendOutcomeSucceeded = 1,
   OxideSendOutcomeFailed = 2,
   OxideSendOutcomeTimedOut = 3,
};

static dispatch_queue_t quic_queue(void)
{
   static dispatch_queue_t queue;
   static dispatch_once_t once;
   dispatch_once(&once, ^{
      queue =
          dispatch_queue_create("com.oxide.platform.network.quic",
                                DISPATCH_QUEUE_SERIAL);
      dispatch_queue_set_specific(queue, &kOxideQuicQueueContext,
                                  &kOxideQuicQueueContext, NULL);
   });
   return queue;
}

static void quic_sync(dispatch_block_t block)
{
   // Session fields are queue-confined; callbacks execute inline to avoid a
   // recursive dispatch_sync deadlock.
   if (dispatch_get_specific(&kOxideQuicQueueContext) ==
       &kOxideQuicQueueContext)
   {
      block();
      return;
   }
   dispatch_sync(quic_queue(), block);
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

static BOOL network_debug_enabled(void)
{
   return env_truthy("OXIDE_NETWORK_DEBUG_LOG");
}

static uint64_t monotonic_deadline_after_ms(uint64_t timeoutMs)
{
   uint64_t nowNs = clock_gettime_nsec_np(CLOCK_MONOTONIC);
   if (timeoutMs > (UINT64_MAX - nowNs) / NSEC_PER_MSEC)
   {
      return UINT64_MAX;
   }
   return nowNs + timeoutMs * NSEC_PER_MSEC;
}

static uint64_t monotonic_remaining_ns(uint64_t deadlineNs)
{
   uint64_t nowNs = clock_gettime_nsec_np(CLOCK_MONOTONIC);
   return deadlineNs > nowNs ? deadlineNs - nowNs : 0;
}

static dispatch_time_t dispatch_deadline_for_remaining(uint64_t remainingNs)
{
   int64_t deltaNs = remainingNs > INT64_MAX ? INT64_MAX
                                              : (int64_t)remainingNs;
   return dispatch_time(DISPATCH_TIME_NOW, deltaNs);
}

static NSString *network_transport_name(BOOL fallback)
{
   return fallback ? @"tcp_tls" : @"quic";
}

static NSString *network_state_name(nw_connection_state_t state)
{
   switch (state)
   {
   case nw_connection_state_invalid:
      return @"invalid";
   case nw_connection_state_waiting:
      return @"waiting";
   case nw_connection_state_preparing:
      return @"preparing";
   case nw_connection_state_ready:
      return @"ready";
   case nw_connection_state_failed:
      return @"failed";
   case nw_connection_state_cancelled:
      return @"cancelled";
   default:
      return @"unknown";
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
     if (type == nw_interface_type_wired)
     {
        kind = 2;
        return false;
     }
     return true;
   });
   return kind;
}

static NSArray *copy_trust_anchors(const struct OxideQuicTlsConfig *tls)
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
      const struct OxideTlsTrustAnchor anchor = tls->trust_anchors[index];
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

static SecIdentityRef copy_identity(const struct OxideQuicTlsConfig *tls)
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
                                  const struct OxideQuicTlsConfig *tls,
                                  const char *server_name,
                                  NSString *alpn,
                                  BOOL tls13Only)
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

   sec_protocol_options_set_tls_tickets_enabled(sec_options, true);
   sec_protocol_options_set_tls_resumption_enabled(sec_options, true);
   if (tls13Only)
   {
      sec_protocol_options_set_min_tls_protocol_version(
          sec_options, tls_protocol_version_TLSv13);
      sec_protocol_options_set_max_tls_protocol_version(
          sec_options, tls_protocol_version_TLSv13);
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

@interface OxideQuicConnection : NSObject
{
   nw_connection_t _connection;
   nw_parameters_t _quicParameters;
   nw_parameters_t _tlsParameters;
}

@property(nonatomic, strong, readonly) dispatch_queue_t queue;
@property(nonatomic, assign) struct OxideQuicConfig quicConfig;
@property(nonatomic, assign) struct OxideQuicRetryPolicy retryPolicy;
@property(nonatomic, assign) struct OxideQuicMetrics metrics;
@property(nonatomic, assign) BOOL fallbackUsed;
@property(nonatomic, assign) BOOL ready;
@property(nonatomic, assign) nw_connection_state_t state;
@property(nonatomic, strong, readonly) dispatch_semaphore_t readySignal;
@property(nonatomic, strong, readonly) dispatch_semaphore_t receiveSignal;
@property(nonatomic, assign) NSUInteger attempt;
@property(nonatomic, assign) BOOL closed;
@property(nonatomic, assign) BOOL currentFallback;
@property(nonatomic, copy) NSString *host;
@property(nonatomic, assign) uint16_t port;
@property(nonatomic, strong) NSDate *handshakeStart;
@property(nonatomic, strong) NSMutableArray<NSData *> *receiveBuffer;
@property(nonatomic, assign) NSUInteger queuedReceiveBytes;
@property(nonatomic, strong) NSMutableData *incomingBytes;

- (instancetype)initWithEndpoint:(NSString *)endpoint
                            port:(uint16_t)port
                             cfg:(const struct OxideQuicConfig *)cfg
                           retry:(const struct OxideQuicRetryPolicy *)retry
                             tls:(const struct OxideQuicTlsConfig *)tls;
- (void)start;
- (void)close;
- (BOOL)copyMetrics:(struct OxideQuicMetrics *)outMetrics;

@end

@interface OxideQuicConnection ()

- (void)startAttemptWithParameters:(nw_parameters_t)parameters
                          fallback:(BOOL)fallback;
- (void)handleReady;
- (void)handleFailure:(nw_error_t)error
              fallback:(BOOL)attemptedFallback
              terminal:(BOOL)terminalEvent;
- (void)scheduleRetryWithParameters:(nw_parameters_t)parameters
                            fallback:(BOOL)fallback;
- (void)configureReadyKeepalive;
- (void)startReceiveLoop;
- (void)drainIncomingBytes;
- (BOOL)waitForReady:(uint64_t)timeoutMs;
- (BOOL)isWritableOnQueue;
- (BOOL)waitForWritableConnection:(uint64_t)deadlineNs;
- (BOOL)sendBytes:(const uint8_t *)data
            length:(size_t)len
           timeout:(uint64_t)timeoutMs;
- (NSData *)popReceived:(uint64_t)timeoutMs;
- (void)closeOnQueue;
- (BOOL)copyClosedState;

@end

@implementation OxideQuicConnection

- (instancetype)initWithEndpoint:(NSString *)endpoint
                            port:(uint16_t)port
                             cfg:(const struct OxideQuicConfig *)cfg
                           retry:(const struct OxideQuicRetryPolicy *)retry
                             tls:(const struct OxideQuicTlsConfig *)tls
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
   uint32_t idleTimeoutMs = _quicConfig.idle_timeout_ms;
   uint16_t maxUdpPayloadSize = _quicConfig.max_datagram_size;
   uint16_t keepaliveIntervalSecs = _quicConfig.keepalive_interval_secs;
   _metrics = (struct OxideQuicMetrics){0};
   _receiveBuffer = [[NSMutableArray alloc] init];
   _queuedReceiveBytes = 0;
   _incomingBytes = [[NSMutableData alloc] init];
   _ready = NO;
   _state = nw_connection_state_invalid;
   _readySignal = dispatch_semaphore_create(0);
   _receiveSignal = dispatch_semaphore_create(0);

   _quicParameters = nw_parameters_create_quic(^(
       nw_protocol_options_t quic_options) {
     sec_protocol_options_t sec_options =
         nw_quic_copy_sec_protocol_options(quic_options);
     configure_sec_options(sec_options, tls, endpoint.UTF8String, alpn, NO);
     nw_quic_set_idle_timeout(quic_options, idleTimeoutMs);
     nw_quic_set_max_udp_payload_size(quic_options, maxUdpPayloadSize);
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
     configure_sec_options(sec_options, tls, endpoint.UTF8String, alpn, YES);
   }, ^(nw_protocol_options_t tcp_options) {
     nw_tcp_options_set_enable_fast_open(tcp_options, true);
     if (keepaliveIntervalSecs > 0)
     {
        nw_tcp_options_set_enable_keepalive(tcp_options, true);
        nw_tcp_options_set_keepalive_idle_time(tcp_options, keepaliveIntervalSecs);
        nw_tcp_options_set_keepalive_interval(tcp_options, keepaliveIntervalSecs);
     }
   });
   if (_tlsParameters != NULL)
   {
      nw_parameters_set_prefer_no_proxy(_tlsParameters, true);
      nw_parameters_set_reuse_local_address(_tlsParameters, true);
      nw_parameters_set_fast_open_enabled(_tlsParameters, true);
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
   quic_sync(^{
     if (self.closed)
     {
        return;
     }
     if (self.quicConfig.force_tcp_tls && self->_tlsParameters != NULL)
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
   [self.receiveBuffer removeAllObjects];
   self.queuedReceiveBytes = 0;
   [self.incomingBytes setLength:0];

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
   self.state = nw_connection_state_invalid;

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
   self.state = nw_connection_state_preparing;
   self.currentFallback = fallback;
   nw_connection_set_queue(_connection, self.queue);
   NSUInteger attemptNumber = self.attempt;
   if (network_debug_enabled())
   {
      NSLog(@"Oxide network connect attempt transport=%@ endpoint=%@:%u "
            @"attempt=%lu",
            network_transport_name(fallback), self.host, self.port,
            (unsigned long)attemptNumber);
   }

   __weak typeof(self) weakSelf = self;
   nw_connection_set_state_changed_handler(
       _connection, ^(nw_connection_state_t state, nw_error_t error) {
         __strong typeof(self) strongSelf = weakSelf;
         if (!strongSelf)
         {
            return;
         }
         if (strongSelf->_connection != connection)
         {
            return;
         }
         strongSelf.state = state;
         if (network_debug_enabled())
         {
            NSLog(@"Oxide network state transport=%@ endpoint=%@:%u "
                  @"attempt=%lu state=%@ error=%@",
                  network_transport_name(fallback), strongSelf.host,
                  strongSelf.port, (unsigned long)attemptNumber,
                  network_state_name(state), error);
         }
         switch (state)
         {
         case nw_connection_state_ready:
            [strongSelf handleReady];
            break;
         case nw_connection_state_invalid:
         case nw_connection_state_failed:
         case nw_connection_state_cancelled:
            [strongSelf handleFailure:error
                              fallback:fallback
                              terminal:YES];
            break;
         case nw_connection_state_waiting:
            [strongSelf handleFailure:error
                              fallback:fallback
                              terminal:NO];
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
   [self configureReadyKeepalive];
   [self startReceiveLoop];
}

- (void)configureReadyKeepalive
{
   if (self.currentFallback || self.quicConfig.keepalive_interval_secs == 0 ||
       _connection == NULL)
   {
      return;
   }

   nw_protocol_definition_t quicDefinition = nw_protocol_copy_quic_definition();
   if (quicDefinition == NULL)
   {
      return;
   }
   nw_protocol_metadata_t quicMetadata =
       nw_connection_copy_protocol_metadata(_connection, quicDefinition);
   if (quicMetadata != NULL)
   {
      nw_quic_set_keepalive_interval(quicMetadata,
                                     self.quicConfig.keepalive_interval_secs);
   }
}

- (void)handleFailure:(nw_error_t)error
              fallback:(BOOL)attemptedFallback
              terminal:(BOOL)terminalEvent
{
   if (self.closed)
   {
      return;
   }

   BOOL canRetry = self.attempt < self.retryPolicy.max_attempts;
   BOOL connectFailed = !self.ready;
   BOOL forceTcpTls = self.quicConfig.force_tcp_tls;
   if (forceTcpTls && _tlsParameters != NULL && canRetry && connectFailed)
   {
      [self scheduleRetryWithParameters:_tlsParameters fallback:YES];
      return;
   }

   if (!attemptedFallback && connectFailed && self.quicConfig.allow_fallback &&
       _tlsParameters != NULL)
   {
      [self scheduleRetryWithParameters:_tlsParameters fallback:YES];
      return;
   }

   if (connectFailed && canRetry)
   {
      [self scheduleRetryWithParameters:_quicParameters fallback:NO];
      return;
   }

   if (error != NULL)
   {
      NSLog(@"Oxide network connection failed transport=%@: %@",
            network_transport_name(self.currentFallback), error);
   }
   if (terminalEvent)
   {
      [self close];
      return;
   }
   self.ready = NO;
   dispatch_semaphore_signal(self.readySignal);
   dispatch_semaphore_signal(self.receiveSignal);
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
   NSUInteger expectedAttempt = self.attempt;
   __weak typeof(self) weakSelf = self;
   dispatch_after(deadline, self.queue, ^{
     __strong typeof(self) strongSelf = weakSelf;
     if (!strongSelf || strongSelf.closed)
     {
        return;
     }
     if (strongSelf.attempt != expectedAttempt || strongSelf.ready)
     {
        if (network_debug_enabled())
        {
           NSLog(@"Oxide network retry skipped transport=%@ "
                 @"expected_attempt=%lu current_attempt=%lu ready=%d",
                 network_transport_name(fallback),
                 (unsigned long)expectedAttempt,
                 (unsigned long)strongSelf.attempt,
                 strongSelf.ready ? 1 : 0);
        }
        return;
     }
     [strongSelf startAttemptWithParameters:parameters fallback:fallback];
   });
}

- (BOOL)waitForReady:(uint64_t)timeoutMs
{
   uint64_t deadlineNs = monotonic_deadline_after_ms(timeoutMs);
   __block BOOL ready = NO;
   __block BOOL closed = NO;
   __block BOOL fallback = NO;
   while (YES)
   {
      quic_sync(^{
        ready = self.ready;
        closed = self.closed;
        fallback = self.currentFallback;
      });
      if (ready)
      {
         return YES;
      }
      if (closed)
      {
         return NO;
      }

      uint64_t remainingNs = monotonic_remaining_ns(deadlineNs);
      if (remainingNs == 0)
      {
         break;
      }
      dispatch_time_t deadline =
          dispatch_deadline_for_remaining(remainingNs);
      dispatch_semaphore_wait(_readySignal, deadline);
   }

   if (network_debug_enabled())
   {
      NSLog(@"Oxide network wait ready timeout transport=%@ timeout_ms=%llu "
            @"ready=%d",
            network_transport_name(fallback),
            (unsigned long long)timeoutMs, ready ? 1 : 0);
   }
   return NO;
}

- (BOOL)isWritableOnQueue
{
   return !self.closed && _connection != NULL &&
          (self.ready || (self.currentFallback &&
                          self.state == nw_connection_state_preparing));
}

- (BOOL)waitForWritableConnection:(uint64_t)deadlineNs
{
   __block BOOL writable = NO;
   __block BOOL closed = NO;
   __block BOOL ready = NO;
   __block BOOL fallback = NO;
   while (YES)
   {
      quic_sync(^{
        writable = [self isWritableOnQueue];
        closed = self.closed;
        ready = self.ready;
        fallback = self.currentFallback;
      });
      if (writable)
      {
         return YES;
      }
      if (closed)
      {
         return NO;
      }

      uint64_t remainingNs = monotonic_remaining_ns(deadlineNs);
      if (remainingNs == 0)
      {
         break;
      }
      uint64_t sliceNs = MIN(remainingNs, 50 * NSEC_PER_MSEC);
      dispatch_time_t sliceDeadline =
          dispatch_deadline_for_remaining(sliceNs);
      dispatch_semaphore_wait(_readySignal, sliceDeadline);
   }

   if (network_debug_enabled())
   {
      NSLog(@"Oxide network wait writable timeout transport=%@ "
            @"ready=%d",
            network_transport_name(fallback), ready ? 1 : 0);
   }
   return NO;
}

- (BOOL)sendBytes:(const uint8_t *)data
            length:(size_t)len
           timeout:(uint64_t)timeoutMs
{
   if (data == NULL || len == 0)
   {
      return NO;
   }
   uint64_t overallDeadlineNs = monotonic_deadline_after_ms(timeoutMs);
   if (![self waitForWritableConnection:overallDeadlineNs])
   {
      return NO;
   }

   __block enum OxideSendOutcome outcome = OxideSendOutcomePending;
   __block nw_connection_t sendConnection = NULL;
   __block BOOL started = NO;
   dispatch_semaphore_t sem = dispatch_semaphore_create(0);
   dispatch_data_t content =
       dispatch_data_create(data, len, NULL, DISPATCH_DATA_DESTRUCTOR_DEFAULT);
   // Admission and completion share the callback queue so timeout has one
   // winner and can only cancel the connection that accepted this send.
   quic_sync(^{
     if (monotonic_remaining_ns(overallDeadlineNs) == 0 ||
         ![self isWritableOnQueue])
     {
        return;
     }

     sendConnection = self->_connection;
     started = YES;
     nw_connection_send(
         sendConnection, content, NW_CONNECTION_DEFAULT_MESSAGE_CONTEXT, true,
         ^(nw_error_t error) {
           if (outcome != OxideSendOutcomePending)
           {
              dispatch_semaphore_signal(sem);
              return;
           }
           if (monotonic_remaining_ns(overallDeadlineNs) == 0)
           {
              outcome = OxideSendOutcomeTimedOut;
              if (self->_connection == sendConnection)
              {
                 [self closeOnQueue];
              }
           }
           else if (error == NULL)
           {
              outcome = OxideSendOutcomeSucceeded;
           }
           else
           {
              outcome = OxideSendOutcomeFailed;
              if (network_debug_enabled())
              {
                 NSLog(@"Oxide network send error transport=%@ "
                       @"len=%zu error=%@",
                       network_transport_name(self.currentFallback), len,
                       error);
              }
           }
           dispatch_semaphore_signal(sem);
         });
   });
   if (!started)
   {
      return NO;
   }

   uint64_t remainingNs = monotonic_remaining_ns(overallDeadlineNs);
   dispatch_time_t deadline = dispatch_deadline_for_remaining(remainingNs);
   if (dispatch_semaphore_wait(sem, deadline) != 0)
   {
      quic_sync(^{
        if (outcome == OxideSendOutcomePending)
        {
           outcome = OxideSendOutcomeTimedOut;
           if (self->_connection == sendConnection)
           {
              [self closeOnQueue];
           }
        }
        if (outcome == OxideSendOutcomeTimedOut && network_debug_enabled())
        {
           NSLog(@"Oxide network send timeout transport=%@ len=%zu "
                 @"timeout_ms=%llu",
                 network_transport_name(self.currentFallback), len,
                 (unsigned long long)timeoutMs);
        }
      });
   }
   return outcome == OxideSendOutcomeSucceeded;
}

- (NSData *)popReceived:(uint64_t)timeoutMs
{
   uint64_t deadlineNs = monotonic_deadline_after_ms(timeoutMs);
   dispatch_time_t deadline =
       dispatch_deadline_for_remaining(monotonic_remaining_ns(deadlineNs));
   if (dispatch_semaphore_wait(_receiveSignal, deadline) != 0)
   {
      return nil;
   }

   __block NSData *after = nil;
   __weak typeof(self) weakSelf = self;
   quic_sync(^{
     __strong typeof(self) strongSelf = weakSelf;
     if (strongSelf && strongSelf.receiveBuffer.count > 0)
     {
        after = strongSelf.receiveBuffer.firstObject;
        [strongSelf.receiveBuffer removeObjectAtIndex:0];
        strongSelf.queuedReceiveBytes -= after.length;
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
   nw_connection_t connection = _connection;
   __weak typeof(self) weakSelf = self;
   nw_connection_receive(
       connection, 1, 65536,
       ^(dispatch_data_t content, nw_content_context_t context, bool isComplete,
         nw_error_t receiveError) {
         (void)context;
         __strong typeof(self) strongSelf = weakSelf;
         if (!strongSelf || strongSelf.closed ||
             strongSelf->_connection != connection)
         {
            return;
         }

         if (content != NULL)
         {
            dispatch_data_apply(
                content, ^bool(dispatch_data_t region, size_t offset,
                               const void *buffer, size_t size) {
                  (void)region;
                  (void)offset;
                  if (buffer != NULL && size > 0)
                  {
                     [strongSelf.incomingBytes appendBytes:buffer length:size];
                  }
                  return true;
                });
            [strongSelf drainIncomingBytes];
         }

         if (receiveError != NULL)
         {
            if (network_debug_enabled())
            {
               NSLog(@"Oxide network receive error transport=%@ error=%@",
                     network_transport_name(strongSelf.currentFallback),
                     receiveError);
            }
            [strongSelf close];
            return;
         }

         if (isComplete)
         {
            if (network_debug_enabled())
            {
               NSLog(@"Oxide network receive closed transport=%@",
                     network_transport_name(strongSelf.currentFallback));
            }
            [strongSelf close];
            return;
         }
         [strongSelf startReceiveLoop];
       });
}

- (void)drainIncomingBytes
{
   while (self.incomingBytes.length >= 4)
   {
      const uint8_t *bytes = self.incomingBytes.bytes;
      uint32_t frameLength = ((uint32_t)bytes[0]) |
                             ((uint32_t)bytes[1] << 8) |
                             ((uint32_t)bytes[2] << 16) |
                             ((uint32_t)bytes[3] << 24);
      if (frameLength < 16 || frameLength > kOxideMaxFrameBytes)
      {
         if (network_debug_enabled())
         {
            NSLog(@"Oxide network receive invalid frame length "
                  @"transport=%@ length=%u buffered=%lu",
                  network_transport_name(self.currentFallback), frameLength,
                  (unsigned long)self.incomingBytes.length);
         }
         [self.incomingBytes setLength:0];
         [self close];
         return;
      }
      if (self.incomingBytes.length < frameLength)
      {
         return;
      }

      if (self.receiveBuffer.count >= kOxideMaxQueuedReceiveFrames ||
          self.queuedReceiveBytes > kOxideMaxQueuedReceiveBytes ||
          frameLength >
              kOxideMaxQueuedReceiveBytes - self.queuedReceiveBytes)
      {
         NSLog(@"Oxide network receive queue overflow transport=%@ "
               @"frame_length=%u queued_frames=%lu queued_bytes=%lu",
               network_transport_name(self.currentFallback), frameLength,
               (unsigned long)self.receiveBuffer.count,
               (unsigned long)self.queuedReceiveBytes);
         [self.incomingBytes setLength:0];
         [self close];
         return;
      }

      NSData *frame =
          [self.incomingBytes subdataWithRange:NSMakeRange(0, frameLength)];
      _metrics.payload_bytes += frame.length;
      _metrics.total_bytes += frame.length;
      [self.receiveBuffer addObject:frame];
      self.queuedReceiveBytes += frame.length;
      if (network_debug_enabled())
      {
         NSLog(@"Oxide network receive frame transport=%@ length=%lu "
               @"queued=%lu",
               network_transport_name(self.currentFallback),
               (unsigned long)frame.length,
               (unsigned long)self.receiveBuffer.count);
      }
      [self.incomingBytes replaceBytesInRange:NSMakeRange(0, frameLength)
                                    withBytes:NULL
                                       length:0];
      dispatch_semaphore_signal(self.receiveSignal);
   }
}

- (BOOL)copyMetrics:(struct OxideQuicMetrics *)outMetrics
{
   if (!outMetrics)
   {
      return false;
   }

   __block BOOL ok = NO;
   quic_sync(^{
     *outMetrics = self->_metrics;
     ok = YES;
   });
   return ok;
}

- (void)close
{
   quic_sync(^{
     [self closeOnQueue];
   });
}

- (void)closeOnQueue
{
   if (self.closed)
   {
      return;
   }
   self.ready = NO;
   self.closed = YES;
   self.state = nw_connection_state_cancelled;
   if (_connection != NULL)
   {
      nw_connection_cancel(_connection);
      _connection = NULL;
   }
   dispatch_semaphore_signal(_readySignal);
   dispatch_semaphore_signal(_receiveSignal);
}

- (BOOL)copyClosedState
{
   __block BOOL closed = NO;
   quic_sync(^{
     closed = self.closed;
   });
   return closed;
}

@end

@interface OxideReachabilityMonitor : NSObject
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
- (BOOL)copyStatus:(struct OxideReachabilityStatus *)outStatus;

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

@implementation OxideReachabilityMonitor

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

- (BOOL)copyStatus:(struct OxideReachabilityStatus *)outStatus
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
         if ([hostPart containsString:@":"])
         {
            return NO;
         }
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
   if (portPart.length == 0)
   {
      return NO;
   }
   uint32_t parsed = 0;
   for (NSUInteger index = 0; index < portPart.length; index++)
   {
      unichar digit = [portPart characterAtIndex:index];
      if (digit < '0' || digit > '9')
      {
         return NO;
      }
      parsed = parsed * 10 + (uint32_t)(digit - '0');
      if (parsed > UINT16_MAX)
      {
         return NO;
      }
   }
   if (parsed == 0)
   {
      return NO;
   }

   *host = hostPart;
   *port = (uint16_t)parsed;
   return YES;
}

OxideQuicHandle oxide_ios_quic_connect(
    const char *endpoint,
    const struct OxideQuicConfig *cfg,
    const struct OxideQuicRetryPolicy *retry,
    const struct OxideQuicTlsConfig *tls)
{
   if (cfg == NULL || retry == NULL || retry->max_attempts == 0)
   {
      return NULL;
   }
   NSString *host = nil;
   uint16_t port = 0;
   if (!parse_endpoint(endpoint, &host, &port))
   {
      return NULL;
   }

   OxideQuicConnection *connection = [[OxideQuicConnection alloc]
       initWithEndpoint:host
                   port:port
                    cfg:cfg
                  retry:retry
                    tls:tls];
   if (!connection)
   {
      return NULL;
   }
   [connection start];
   return (void *)CFBridgingRetain(connection);
}

bool oxide_ios_quic_metrics(OxideQuicHandle handle,
                              struct OxideQuicMetrics *outMetrics)
{
   if (handle == NULL || outMetrics == NULL)
   {
      return false;
   }
   OxideQuicConnection *connection =
       (__bridge OxideQuicConnection *)handle;
   return [connection copyMetrics:outMetrics];
}

bool oxide_ios_quic_wait_ready(OxideQuicHandle handle,
                                 uint64_t timeout_ms)
{
   if (handle == NULL)
   {
      return false;
   }
   OxideQuicConnection *connection =
       (__bridge OxideQuicConnection *)handle;
   return [connection waitForReady:timeout_ms];
}

void oxide_ios_quic_close(OxideQuicHandle handle)
{
   if (handle == NULL)
   {
      return;
   }
   OxideQuicConnection *connection =
       (__bridge_transfer OxideQuicConnection *)handle;
   [connection close];
}

bool oxide_ios_quic_send(OxideQuicHandle handle,
                           const uint8_t *data,
                           size_t len,
                           uint64_t timeout_ms)
{
   if (handle == NULL || data == NULL || len == 0)
   {
      return false;
   }
   OxideQuicConnection *connection =
       (__bridge OxideQuicConnection *)handle;
   return [connection sendBytes:data length:len timeout:timeout_ms];
}

bool oxide_ios_quic_recv(OxideQuicHandle handle,
                           uint8_t *buffer,
                           size_t buffer_len,
                           size_t *out_len,
                           uint64_t timeout_ms)
{
   if (handle == NULL || buffer == NULL || out_len == NULL)
   {
      return false;
   }

   OxideQuicConnection *connection =
       (__bridge OxideQuicConnection *)handle;
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

int32_t oxide_ios_quic_poll_recv(OxideQuicHandle handle,
                                   uint8_t *buffer,
                                   size_t buffer_len,
                                   size_t *out_len)
{
   if (out_len != NULL)
   {
      *out_len = 0;
   }
   if (handle == NULL || buffer == NULL || out_len == NULL)
   {
      return OXIDE_IOS_QUIC_POLL_TERMINAL;
   }

   OxideQuicConnection *connection =
       (__bridge OxideQuicConnection *)handle;
   NSData *payload = [connection popReceived:0];
   if (payload == nil)
   {
      return [connection copyClosedState]
                 ? OXIDE_IOS_QUIC_POLL_TERMINAL
                 : OXIDE_IOS_QUIC_POLL_IDLE;
   }
   if (payload.length > buffer_len)
   {
      NSLog(@"Oxide network poll buffer too small frame_length=%lu "
            @"buffer_length=%zu",
            (unsigned long)payload.length, buffer_len);
      [connection close];
      return OXIDE_IOS_QUIC_POLL_TERMINAL;
   }

   memcpy(buffer, payload.bytes, payload.length);
   *out_len = payload.length;
   return OXIDE_IOS_QUIC_POLL_FRAME;
}

OxideReachabilityHandle oxide_ios_reachability_start(void)
{
   OxideReachabilityMonitor *monitor =
       [[OxideReachabilityMonitor alloc] init];
   if (!monitor)
   {
      return NULL;
   }
   [monitor start];
   return (void *)CFBridgingRetain(monitor);
}

bool oxide_ios_reachability_poll(
    OxideReachabilityHandle handle,
    struct OxideReachabilityStatus *outStatus)
{
   if (handle == NULL || outStatus == NULL)
   {
      return false;
   }
   OxideReachabilityMonitor *monitor =
       (__bridge OxideReachabilityMonitor *)handle;
   return [monitor copyStatus:outStatus];
}

void oxide_ios_reachability_close(OxideReachabilityHandle handle)
{
   if (handle == NULL)
   {
      return;
   }
   OxideReachabilityMonitor *monitor =
       (__bridge_transfer OxideReachabilityMonitor *)handle;
   [monitor stop];
}

void oxide_host_net_set_reachability_callback(OxideReachabilityCallback callback)
{
   g_oxide_reachability_callback = callback;
}

int32_t oxide_host_net_start_reachability(void)
{
   if (g_oxide_reachability_monitor == nil)
   {
      g_oxide_reachability_monitor = [[OxideReachabilityMonitor alloc] init];
      if (g_oxide_reachability_monitor == nil)
      {
         return -1;
      }
      [(OxideReachabilityMonitor *)g_oxide_reachability_monitor start];
   }

   struct OxideReachabilityStatus snapshot;
   memset(&snapshot, 0, sizeof(snapshot));
   if ([(OxideReachabilityMonitor *)g_oxide_reachability_monitor
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
      [(OxideReachabilityMonitor *)g_oxide_reachability_monitor stop];
      g_oxide_reachability_monitor = nil;
   }
}
