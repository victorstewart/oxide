#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>
#import <stdatomic.h>
#import <stdint.h>
#import <stdio.h>
#import <stdlib.h>
#import <string.h>

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

typedef void (*OxideHttpCallback)(void *, const struct OxideHttpEvent *);

extern int32_t oxide_host_http_start(
    const uint8_t *, size_t, uint32_t, size_t,
    const struct OxideHttpHeader *, size_t,
    const struct OxideHttpHeader *, size_t,
    OxideHttpCallback, void *, uint64_t *);
extern void oxide_host_http_cancel(uint64_t);
extern void oxide_host_http_test_barrier(dispatch_block_t);

static void require(BOOL condition, const char *message);

@interface OxideHttpTestProtocol : NSURLProtocol
@end

@implementation OxideHttpTestProtocol

+ (BOOL)canInitWithRequest:(NSURLRequest *)request
{
   return [request.URL.host isEqualToString:@"oxide.test"];
}

+ (NSURLRequest *)canonicalRequestForRequest:(NSURLRequest *)request
{
   return request;
}

- (void)startLoading
{
   NSString *path = self.request.URL.path;
   if ([path isEqualToString:@"/cancel"])
   {
      return;
   }

   NSURL *responseURL = self.request.URL;
   NSMutableDictionary<NSString *, NSString *> *headers = [[NSMutableDictionary alloc] init];
   if ([path isEqualToString:@"/declared-over"])
   {
      headers[@"Content-Length"] = @"6";
   }
   else if ([path isEqualToString:@"/streamed-over"])
   {
      headers[@"Transfer-Encoding"] = @"chunked";
   }
   else if ([path isEqualToString:@"/header-value-over"])
   {
      headers[@"X-One"] = [@"a" stringByPaddingToLength:32 * 1024 withString:@"a" startingAtIndex:0];
   }
   else if ([path isEqualToString:@"/header-aggregate-over"])
   {
      NSString *value = [@"a" stringByPaddingToLength:16 * 1024 withString:@"a" startingAtIndex:0];
      headers[@"X-A"] = value;
      headers[@"X-B"] = value;
   }
   else if ([path isEqualToString:@"/final-url-over"])
   {
      NSString *prefix = @"https://oxide.test/";
      NSString *suffix = [@"a" stringByPaddingToLength:16 * 1024 + 1 - prefix.length withString:@"a" startingAtIndex:0];
      responseURL = [NSURL URLWithString:[prefix stringByAppendingString:suffix]];
   }
   else if ([path isEqualToString:@"/exact"])
   {
      headers[@"Content-Length"] = @"5";
      headers[@"X-Exact"] = [@"a" stringByPaddingToLength:32 * 1024 - 7 withString:@"a" startingAtIndex:0];
      NSString *prefix = @"https://oxide.test/";
      NSString *suffix = [@"a" stringByPaddingToLength:16 * 1024 - prefix.length withString:@"a" startingAtIndex:0];
      responseURL = [NSURL URLWithString:[prefix stringByAppendingString:suffix]];
   }

   NSHTTPURLResponse *response = [[NSHTTPURLResponse alloc]
       initWithURL:responseURL statusCode:200 HTTPVersion:@"HTTP/1.1" headerFields:headers];
   [self.client URLProtocol:self didReceiveResponse:response cacheStoragePolicy:NSURLCacheStorageNotAllowed];
   if ([path isEqualToString:@"/streamed-over"])
   {
      [self.client URLProtocol:self didLoadData:[@"abc" dataUsingEncoding:NSUTF8StringEncoding]];
      [self.client URLProtocol:self didLoadData:[@"def" dataUsingEncoding:NSUTF8StringEncoding]];
   }
   else if ([path isEqualToString:@"/exact"])
   {
      [self.client URLProtocol:self didLoadData:[@"ab" dataUsingEncoding:NSUTF8StringEncoding]];
      [self.client URLProtocol:self didLoadData:[@"cde" dataUsingEncoding:NSUTF8StringEncoding]];
   }
   [self.client URLProtocolDidFinishLoading:self];
}

- (void)stopLoading
{
}

@end

struct TestState
{
   dispatch_semaphore_t terminal;
   _Atomic uint32_t responses;
   _Atomic uint32_t bodyEvents;
   _Atomic uint32_t bodyBytes;
   _Atomic uint32_t terminals;
   _Atomic uint32_t terminalKind;
   _Atomic size_t finalURLBytes;
   _Atomic size_t headerCount;
   _Atomic size_t metadataBytes;
};

static void receiveEvent(void *context, const struct OxideHttpEvent *event)
{
   struct TestState *state = context;
   if (event->kind == 1)
   {
      atomic_fetch_add(&state->responses, 1);
      atomic_store(&state->finalURLBytes, event->final_url_len);
      atomic_store(&state->headerCount, event->header_count);
      size_t metadataBytes = 0;
      for (size_t index = 0; index < event->header_count; index += 1)
      {
         metadataBytes += event->headers_ptr[index].name_len;
         metadataBytes += event->headers_ptr[index].value_len;
      }
      atomic_store(&state->metadataBytes, metadataBytes);
   }
   else if (event->kind == 2)
   {
      atomic_fetch_add(&state->bodyEvents, 1);
      atomic_fetch_add(&state->bodyBytes, (uint32_t)event->data_len);
   }
   else
   {
      atomic_fetch_add(&state->terminals, 1);
      atomic_store(&state->terminalKind, event->kind);
      dispatch_semaphore_signal(state->terminal);
   }
}

static void require(BOOL condition, const char *message)
{
   if (!condition)
   {
      fprintf(stderr, "%s\n", message);
      exit(1);
   }
}

static struct OxideHttpHeader selectedHeader(NSString *name)
{
   return (struct OxideHttpHeader){
       .name_ptr = (const uint8_t *)name.UTF8String,
       .name_len = [name lengthOfBytesUsingEncoding:NSUTF8StringEncoding],
   };
}

static struct TestState *runScenario(NSString *path, size_t maximumBytes,
                                     const struct OxideHttpHeader *selected, size_t selectedCount,
                                     uint32_t expectedTerminal, uint32_t expectedResponses,
                                     uint32_t expectedBodyEvents, uint32_t expectedBodyBytes,
                                     BOOL cancel)
{
   NSString *urlString = [@"https://oxide.test" stringByAppendingString:path];
   NSData *url = [urlString dataUsingEncoding:NSUTF8StringEncoding];
   struct TestState *state = calloc(1, sizeof(struct TestState));
   require(state != NULL, "failed to allocate native HTTP test state");
   state->terminal = dispatch_semaphore_create(0);
   uint64_t requestID = 0;
   int32_t result = oxide_host_http_start(
       url.bytes, url.length, 1000, maximumBytes, NULL, 0,
       selected, selectedCount, receiveEvent, state, &requestID);
   require(result == 0 && requestID != 0, "native HTTP scenario was not admitted");
   if (cancel)
   {
      oxide_host_http_cancel(requestID);
   }
   if (dispatch_semaphore_wait(state->terminal,
                               dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC)) != 0)
   {
      fprintf(stderr, "native HTTP scenario did not terminate: %s\n", path.UTF8String);
      exit(1);
   }
   require(atomic_load(&state->terminalKind) == expectedTerminal, "native HTTP scenario emitted the wrong terminal kind");
   require(atomic_load(&state->responses) == expectedResponses, "native HTTP scenario emitted the wrong response count");
   require(atomic_load(&state->bodyEvents) == expectedBodyEvents, "native HTTP scenario emitted the wrong body event count");
   require(atomic_load(&state->bodyBytes) == expectedBodyBytes, "native HTTP scenario emitted the wrong body byte count");
   return state;
}

int main(void)
{
   @autoreleasepool
   {
      require([NSURLProtocol registerClass:[OxideHttpTestProtocol class]], "failed to register local URL protocol");
      struct OxideHttpHeader one = selectedHeader(@"x-one");
      struct OxideHttpHeader aggregate[] = { selectedHeader(@"x-a"), selectedHeader(@"x-b") };
      struct OxideHttpHeader exact = selectedHeader(@"x-exact");

      struct TestState *states[] = {
          runScenario(@"/declared-over", 5, NULL, 0, 5, 0, 0, 0, NO),
          runScenario(@"/streamed-over", 5, NULL, 0, 5, 1, 0, 0, NO),
          runScenario(@"/header-value-over", 5, &one, 1, 5, 0, 0, 0, NO),
          runScenario(@"/header-aggregate-over", 5, aggregate, 2, 5, 0, 0, 0, NO),
          runScenario(@"/final-url-over", 5, NULL, 0, 5, 0, 0, 0, NO),
          runScenario(@"/cancel", 5, NULL, 0, 4, 0, 0, 0, YES),
          runScenario(@"/exact", 5, &exact, 1, 3, 1, 1, 5, NO),
      };

      dispatch_semaphore_t barrier = dispatch_semaphore_create(0);
      oxide_host_http_test_barrier(^{ dispatch_semaphore_signal(barrier); });
      require(dispatch_semaphore_wait(barrier,
                                      dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC)) == 0,
              "native HTTP delegate barrier did not complete");
      for (size_t index = 0; index < sizeof(states) / sizeof(states[0]); index += 1)
      {
         require(atomic_load(&states[index]->terminals) == 1,
                 "native HTTP scenario emitted multiple terminal callbacks");
      }
      struct TestState *exactState = states[6];
      require(atomic_load(&exactState->finalURLBytes) == 16 * 1024,
              "exact native HTTP final URL bound was not preserved");
      require(atomic_load(&exactState->headerCount) == 1,
              "exact native HTTP selected header count was not preserved");
      require(atomic_load(&exactState->metadataBytes) == 32 * 1024,
              "exact native HTTP metadata bound was not preserved");

      struct OxideHttpHeader count[65];
      for (size_t index = 0; index < 65; index += 1)
      {
         count[index] = selectedHeader(@"x");
      }
      NSData *url = [@"https://oxide.test/header-count" dataUsingEncoding:NSUTF8StringEncoding];
      struct TestState state = { .terminal = dispatch_semaphore_create(0) };
      uint64_t requestID = 0;
      require(oxide_host_http_start(url.bytes, url.length, 1000, 5, NULL, 0,
                                    count, 65, receiveEvent, &state, &requestID) == -1,
              "native HTTP FFI accepted too many selected headers");
      require(atomic_load(&state.terminals) == 0, "rejected native HTTP request invoked its callback");
      for (size_t index = 0; index < sizeof(states) / sizeof(states[0]); index += 1)
      {
         states[index]->terminal = nil;
         free(states[index]);
      }
   }
   return 0;
}
