#import <Foundation/Foundation.h>
#import <TargetConditionals.h>
#import <stdatomic.h>
#import <stdbool.h>
#import <stddef.h>
#import <stdint.h>
#import <stdlib.h>

struct OxideHttpHeader
{
   const uint8_t *name_ptr;
   size_t name_len;
   const uint8_t *value_ptr;
   size_t value_len;
};

struct OxideHttpEvent
{
   uint32_t kind;
   int32_t error;
   uint16_t status;
   uint16_t reserved;
   int64_t content_length;
   const uint8_t *data_ptr;
   size_t data_len;
   const uint8_t *final_url_ptr;
   size_t final_url_len;
   const struct OxideHttpHeader *headers_ptr;
   size_t header_count;
};

typedef void (*OxideHttpCallback)(void *context, const struct OxideHttpEvent *event);

_Static_assert(sizeof(struct OxideHttpHeader) == 32,
               "OxideHttpHeader ABI size changed");
_Static_assert(_Alignof(struct OxideHttpHeader) == 8,
               "OxideHttpHeader ABI alignment changed");
_Static_assert(sizeof(struct OxideHttpEvent) == 72,
               "OxideHttpEvent ABI size changed");
_Static_assert(_Alignof(struct OxideHttpEvent) == 8,
               "OxideHttpEvent ABI alignment changed");

enum
{
   OxideHttpEventResponse = 1,
   OxideHttpEventBody = 2,
   OxideHttpEventComplete = 3,
   OxideHttpEventCancelled = 4,
   OxideHttpEventFailed = 5,
};

enum
{
   OxideHttpMaximumHeaderCount = 64,
   OxideHttpMaximumMetadataBytes = 32 * 1024,
   OxideHttpMaximumURLBytes = 16 * 1024,
};

@interface OxideHttpTaskState : NSObject
@property(nonatomic) uint64_t requestID;
@property(nonatomic) size_t maximumBytes;
@property(nonatomic) size_t receivedBytes;
@property(nonatomic) OxideHttpCallback callback;
@property(nonatomic) void *context;
@property(nonatomic, strong) NSArray<NSString *> *selectedHeaders;
@property(nonatomic, strong) NSURLSessionDataTask *task;
@end

@implementation OxideHttpTaskState
@end

@interface OxideHttpDelegate : NSObject <NSURLSessionDataDelegate, NSURLSessionTaskDelegate>
@property(nonatomic, strong) NSLock *lock;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, OxideHttpTaskState *> *requests;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, OxideHttpTaskState *> *tasks;
@property(nonatomic, strong) NSOperationQueue *delegateQueue;
@property(nonatomic, strong) NSURLSession *session;
@end

@implementation OxideHttpDelegate

+ (instancetype)shared
{
   static OxideHttpDelegate *delegate = nil;
   static dispatch_once_t once;
   dispatch_once(&once, ^{
     delegate = [[OxideHttpDelegate alloc] initPrivate];
   });
   return delegate;
}

- (instancetype)initPrivate
{
   self = [super init];
   if (self == nil)
   {
      return nil;
   }

   _lock = [[NSLock alloc] init];
   _requests = [[NSMutableDictionary alloc] init];
   _tasks = [[NSMutableDictionary alloc] init];

   NSURLSessionConfiguration *configuration =
       [NSURLSessionConfiguration ephemeralSessionConfiguration];
#if defined(OXIDE_HTTP_TESTING)
   Class testProtocol = NSClassFromString(@"OxideHttpTestProtocol");
   if (testProtocol != nil)
   {
      configuration.protocolClasses = @[ testProtocol ];
   }
#endif
   configuration.HTTPCookieStorage = nil;
   configuration.URLCredentialStorage = nil;
   configuration.URLCache = nil;
   configuration.HTTPShouldSetCookies = NO;
   configuration.requestCachePolicy = NSURLRequestReloadIgnoringLocalCacheData;
   configuration.waitsForConnectivity = YES;
#if TARGET_OS_IPHONE
   configuration.allowsCellularAccess = YES;
   configuration.multipathServiceType = NSURLSessionMultipathServiceTypeHandover;
#endif

   NSOperationQueue *queue = [[NSOperationQueue alloc] init];
   queue.maxConcurrentOperationCount = 1;
   queue.qualityOfService = NSQualityOfServiceUserInitiated;
   _delegateQueue = queue;
   _session = [NSURLSession sessionWithConfiguration:configuration
                                            delegate:self
                                       delegateQueue:queue];
   return self;
}

- (instancetype)init
{
   return [OxideHttpDelegate shared];
}

- (void)registerState:(OxideHttpTaskState *)state
{
   [self.lock lock];
   self.requests[@(state.requestID)] = state;
   self.tasks[@(state.task.taskIdentifier)] = state;
   [self.lock unlock];
}

- (OxideHttpTaskState *)stateForTask:(NSURLSessionTask *)task
{
   [self.lock lock];
   OxideHttpTaskState *state = self.tasks[@(task.taskIdentifier)];
   [self.lock unlock];
   return state;
}

- (OxideHttpTaskState *)takeStateForRequest:(uint64_t)requestID
{
   [self.lock lock];
   OxideHttpTaskState *state = self.requests[@(requestID)];
   if (state != nil)
   {
      [self.requests removeObjectForKey:@(requestID)];
      [self.tasks removeObjectForKey:@(state.task.taskIdentifier)];
   }
   [self.lock unlock];
   return state;
}

- (OxideHttpTaskState *)takeStateForTask:(NSURLSessionTask *)task
{
   [self.lock lock];
   OxideHttpTaskState *state = self.tasks[@(task.taskIdentifier)];
   if (state != nil)
   {
      [self.tasks removeObjectForKey:@(task.taskIdentifier)];
      [self.requests removeObjectForKey:@(state.requestID)];
   }
   [self.lock unlock];
   return state;
}

- (void)deliverTerminal:(OxideHttpTaskState *)state kind:(uint32_t)kind error:(int32_t)error
{
   if (state == nil || state.callback == NULL)
   {
      return;
   }
   struct OxideHttpEvent event = {
       .kind = kind,
       .error = error,
       .content_length = -1,
   };
   state.callback(state.context, &event);
}

- (void)failTask:(NSURLSessionTask *)task error:(int32_t)error
{
   OxideHttpTaskState *state = [self takeStateForTask:task];
   if (state != nil)
   {
      [task cancel];
      [self deliverTerminal:state kind:OxideHttpEventFailed error:error];
   }
}

- (void)cancelRequest:(uint64_t)requestID
{
   OxideHttpTaskState *state = [self takeStateForRequest:requestID];
   if (state != nil)
   {
      [state.task cancel];
      [self deliverTerminal:state kind:OxideHttpEventCancelled error:0];
   }
}

- (void)timeoutRequest:(uint64_t)requestID
{
   OxideHttpTaskState *state = [self takeStateForRequest:requestID];
   if (state != nil)
   {
      [state.task cancel];
      [self deliverTerminal:state kind:OxideHttpEventFailed error:-2];
   }
}

- (BOOL)deliverResponse:(NSHTTPURLResponse *)response state:(OxideHttpTaskState *)state
{
   if (state == nil || state.callback == NULL)
   {
      return NO;
   }

   const int64_t declaredLength = response.expectedContentLength;
   if (declaredLength > 0 && (uint64_t)declaredLength > state.maximumBytes)
   {
      [self failTask:state.task error:-4];
      return NO;
   }

   NSString *finalURLString = response.URL.absoluteString;
   const NSUInteger finalURLBytes =
       [finalURLString lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
   if (finalURLString == nil)
   {
      [self failTask:state.task error:-5];
      return NO;
   }
   if (finalURLBytes > OxideHttpMaximumURLBytes)
   {
      [self failTask:state.task error:-4];
      return NO;
   }

   NSUInteger metadataBytes = 0;
   NSUInteger count = 0;
   for (NSString *name in state.selectedHeaders)
   {
      NSString *value = [response valueForHTTPHeaderField:name];
      if (value == nil)
      {
         continue;
      }
      const NSUInteger nameBytes =
          [name lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
      const NSUInteger valueBytes =
          [value lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
      if (count >= OxideHttpMaximumHeaderCount ||
          nameBytes > OxideHttpMaximumMetadataBytes - metadataBytes ||
          valueBytes > OxideHttpMaximumMetadataBytes - metadataBytes - nameBytes)
      {
         [self failTask:state.task error:-4];
         return NO;
      }
      metadataBytes += nameBytes + valueBytes;
      count += 1;
   }

   NSData *finalURL = [finalURLString dataUsingEncoding:NSUTF8StringEncoding];
   if (finalURL == nil)
   {
      [self failTask:state.task error:-5];
      return NO;
   }
   struct OxideHttpHeader *headers =
       count == 0 ? NULL : calloc(count, sizeof(struct OxideHttpHeader));
   if (count != 0 && headers == NULL)
   {
      [self failTask:state.task error:-5];
      return NO;
   }

   NSMutableArray<NSData *> *names = [[NSMutableArray alloc] initWithCapacity:count];
   NSMutableArray<NSData *> *values = [[NSMutableArray alloc] initWithCapacity:count];
   size_t emitted = 0;
   for (NSString *name in state.selectedHeaders)
   {
      NSString *value = [response valueForHTTPHeaderField:name];
      if (value == nil)
      {
         continue;
      }
      NSData *nameData = [name dataUsingEncoding:NSUTF8StringEncoding];
      NSData *valueData = [value dataUsingEncoding:NSUTF8StringEncoding];
      if (nameData == nil || valueData == nil)
      {
         free(headers);
         [self failTask:state.task error:-5];
         return NO;
      }
      [names addObject:nameData];
      [values addObject:valueData];
      headers[emitted] = (struct OxideHttpHeader){
          .name_ptr = nameData.bytes,
          .name_len = nameData.length,
          .value_ptr = valueData.bytes,
          .value_len = valueData.length,
      };
      emitted += 1;
   }

   struct OxideHttpEvent event = {
       .kind = OxideHttpEventResponse,
       .status = (uint16_t)response.statusCode,
       .content_length = declaredLength,
       .final_url_ptr = finalURL.bytes,
       .final_url_len = finalURL.length,
       .headers_ptr = headers,
       .header_count = emitted,
   };
   state.callback(state.context, &event);
   free(headers);
   return YES;
}

- (void)URLSession:(NSURLSession *)session
              task:(NSURLSessionTask *)task
willPerformHTTPRedirection:(NSHTTPURLResponse *)response
        newRequest:(NSURLRequest *)request
 completionHandler:(void (^)(NSURLRequest *))completionHandler
{
   (void)session;
   (void)request;
   OxideHttpTaskState *state = [self stateForTask:task];
   if ([self deliverResponse:response state:state])
   {
      state = [self takeStateForTask:task];
      [self deliverTerminal:state kind:OxideHttpEventComplete error:0];
   }
   completionHandler(nil);
}

- (void)URLSession:(NSURLSession *)session
          dataTask:(NSURLSessionDataTask *)dataTask
didReceiveResponse:(NSURLResponse *)response
 completionHandler:(void (^)(NSURLSessionResponseDisposition))completionHandler
{
   (void)session;
   if (![response isKindOfClass:[NSHTTPURLResponse class]])
   {
      [self failTask:dataTask error:-3];
      completionHandler(NSURLSessionResponseCancel);
      return;
   }
   OxideHttpTaskState *state = [self stateForTask:dataTask];
   completionHandler([self deliverResponse:(NSHTTPURLResponse *)response state:state]
                         ? NSURLSessionResponseAllow
                         : NSURLSessionResponseCancel);
}

- (void)URLSession:(NSURLSession *)session
          dataTask:(NSURLSessionDataTask *)dataTask
    didReceiveData:(NSData *)data
{
   (void)session;
   OxideHttpTaskState *state = [self stateForTask:dataTask];
   if (state == nil)
   {
      return;
   }
   if (data.length > state.maximumBytes - state.receivedBytes)
   {
      [self failTask:dataTask error:-4];
      return;
   }
   state.receivedBytes += data.length;
   struct OxideHttpEvent event = {
       .kind = OxideHttpEventBody,
       .content_length = -1,
       .data_ptr = data.bytes,
       .data_len = data.length,
   };
   state.callback(state.context, &event);
}

- (void)URLSession:(NSURLSession *)session
              task:(NSURLSessionTask *)task
didCompleteWithError:(NSError *)error
{
   (void)session;
   OxideHttpTaskState *state = [self takeStateForTask:task];
   if (state == nil)
   {
      return;
   }
   [self deliverTerminal:state
                    kind:error == nil ? OxideHttpEventComplete : OxideHttpEventFailed
                   error:error == nil ? 0 : -2];
}

- (void)URLSession:(NSURLSession *)session
              task:(NSURLSessionTask *)task
didReceiveChallenge:(NSURLAuthenticationChallenge *)challenge
 completionHandler:(void (^)(NSURLSessionAuthChallengeDisposition, NSURLCredential *))completionHandler
{
   (void)session;
   (void)task;
   if ([challenge.protectionSpace.authenticationMethod
          isEqualToString:NSURLAuthenticationMethodServerTrust])
   {
      completionHandler(NSURLSessionAuthChallengePerformDefaultHandling, nil);
   }
   else
   {
      completionHandler(NSURLSessionAuthChallengeCancelAuthenticationChallenge, nil);
   }
}

@end

static bool oxide_http_header_name_valid(NSString *name)
{
   if (name.length == 0)
   {
      return false;
   }
   NSCharacterSet *allowed = [NSCharacterSet
       characterSetWithCharactersInString:
           @"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-.^_`|~"];
   return [name rangeOfCharacterFromSet:allowed.invertedSet].location == NSNotFound;
}

int32_t oxide_host_http_start(const uint8_t *url_ptr,
                              size_t url_len,
                              uint32_t timeout_ms,
                              size_t max_response_bytes,
                              const struct OxideHttpHeader *request_headers,
                              size_t request_header_count,
                              const struct OxideHttpHeader *response_headers,
                              size_t response_header_count,
                              OxideHttpCallback callback,
                              void *context,
                              uint64_t *out_request_id)
{
   if (url_ptr == NULL || url_len == 0 || timeout_ms == 0 ||
       max_response_bytes == 0 || callback == NULL || context == NULL ||
       out_request_id == NULL ||
       (request_header_count != 0 && request_headers == NULL) ||
       (response_header_count != 0 && response_headers == NULL) ||
       (request_headers != NULL &&
        (uintptr_t)request_headers % _Alignof(struct OxideHttpHeader) != 0) ||
       (response_headers != NULL &&
        (uintptr_t)response_headers % _Alignof(struct OxideHttpHeader) != 0) ||
       url_len > OxideHttpMaximumURLBytes ||
       request_header_count > OxideHttpMaximumHeaderCount ||
       response_header_count > OxideHttpMaximumHeaderCount - request_header_count)
   {
      return -1;
   }

   size_t metadataBytes = 0;
   for (size_t index = 0; index < request_header_count; index += 1)
   {
      const struct OxideHttpHeader *raw = &request_headers[index];
      if (raw->name_ptr == NULL || raw->name_len == 0 ||
          (raw->value_len != 0 && raw->value_ptr == NULL) ||
          raw->name_len > OxideHttpMaximumMetadataBytes - metadataBytes)
      {
         return -1;
      }
      metadataBytes += raw->name_len;
      if (raw->value_len > OxideHttpMaximumMetadataBytes - metadataBytes)
      {
         return -1;
      }
      metadataBytes += raw->value_len;
   }
   for (size_t index = 0; index < response_header_count; index += 1)
   {
      const struct OxideHttpHeader *raw = &response_headers[index];
      if (raw->name_ptr == NULL || raw->name_len == 0 ||
          raw->value_ptr != NULL || raw->value_len != 0 ||
          raw->name_len > OxideHttpMaximumMetadataBytes - metadataBytes)
      {
         return -1;
      }
      metadataBytes += raw->name_len;
   }

   NSString *urlString = [[NSString alloc] initWithBytes:url_ptr
                                                 length:url_len
                                               encoding:NSUTF8StringEncoding];
   NSURL *url = urlString == nil ? nil : [NSURL URLWithString:urlString];
   NSString *scheme = url.scheme.lowercaseString;
   if (url == nil || url.host.length == 0 || url.user.length != 0 || url.password.length != 0 ||
       !([scheme isEqualToString:@"https"] || [scheme isEqualToString:@"http"]))
   {
      return -1;
   }

   NSTimeInterval timeout = (NSTimeInterval)timeout_ms / 1000.0;
   NSMutableURLRequest *request =
       [NSMutableURLRequest requestWithURL:url
                              cachePolicy:NSURLRequestReloadIgnoringLocalCacheData
                          timeoutInterval:timeout];
   request.HTTPMethod = @"GET";
   request.HTTPShouldHandleCookies = NO;

   for (size_t index = 0; index < request_header_count; index += 1)
   {
      const struct OxideHttpHeader *raw = &request_headers[index];
      NSString *name = [[NSString alloc] initWithBytes:raw->name_ptr
                                               length:raw->name_len
                                             encoding:NSUTF8StringEncoding];
      NSString *value = [[NSString alloc] initWithBytes:raw->value_ptr
                                                length:raw->value_len
                                              encoding:NSUTF8StringEncoding];
      NSString *lower = name.lowercaseString;
      if (!oxide_http_header_name_valid(name) || value == nil ||
          [value rangeOfCharacterFromSet:[NSCharacterSet newlineCharacterSet]].location != NSNotFound ||
          [lower isEqualToString:@"cookie"] ||
          [lower isEqualToString:@"authorization"] ||
          [lower isEqualToString:@"proxy-authorization"])
      {
         return -1;
      }
      [request setValue:value forHTTPHeaderField:name];
   }

   NSMutableArray<NSString *> *selected =
       [[NSMutableArray alloc] initWithCapacity:response_header_count];
   for (size_t index = 0; index < response_header_count; index += 1)
   {
      const struct OxideHttpHeader *raw = &response_headers[index];
      NSString *name = [[NSString alloc] initWithBytes:raw->name_ptr
                                               length:raw->name_len
                                             encoding:NSUTF8StringEncoding];
      if (!oxide_http_header_name_valid(name))
      {
         return -1;
      }
      [selected addObject:name];
   }

   static _Atomic uint64_t nextRequestID = 1;
   uint64_t requestID = atomic_fetch_add_explicit(
       &nextRequestID, 1, memory_order_relaxed);
   if (requestID == 0)
   {
      abort();
   }

   OxideHttpDelegate *delegate = [OxideHttpDelegate shared];
   NSURLSessionDataTask *task = [delegate.session dataTaskWithRequest:request];
   if (task == nil)
   {
      return -2;
   }
   OxideHttpTaskState *state = [[OxideHttpTaskState alloc] init];
   state.requestID = requestID;
   state.maximumBytes = max_response_bytes;
   state.callback = callback;
   state.context = context;
   state.selectedHeaders = selected;
   state.task = task;
   [delegate registerState:state];
   *out_request_id = requestID;
   [task resume];

   dispatch_after(
       dispatch_time(DISPATCH_TIME_NOW, (int64_t)timeout_ms * NSEC_PER_MSEC),
       dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
         OxideHttpDelegate *delegate = [OxideHttpDelegate shared];
         [delegate.delegateQueue addOperationWithBlock:^{
           [delegate timeoutRequest:requestID];
         }];
       });
   return 0;
}

void oxide_host_http_cancel(uint64_t request_id)
{
   if (request_id != 0)
   {
      OxideHttpDelegate *delegate = [OxideHttpDelegate shared];
      [delegate.delegateQueue addOperationWithBlock:^{
        [delegate cancelRequest:request_id];
      }];
   }
}

#if defined(OXIDE_HTTP_TESTING)
void oxide_host_http_test_barrier(dispatch_block_t block)
{
   [[[OxideHttpDelegate shared] delegateQueue] addOperationWithBlock:block];
}
#endif
