#import <Foundation/Foundation.h>
#import <TargetConditionals.h>
#import <dispatch/dispatch.h>
#import <stdbool.h>
#import <stddef.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

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

_Static_assert(sizeof(struct OxideHttpResponse) == 56,
               "OxideHttpResponse ABI size changed");
_Static_assert(_Alignof(struct OxideHttpResponse) == 8,
               "OxideHttpResponse ABI alignment changed");

void oxide_host_http_response_free(struct OxideHttpResponse *response);

static void oxide_http_response_clear(struct OxideHttpResponse *response)
{
   if (response != NULL)
   {
      memset(response, 0, sizeof(*response));
   }
}

static bool oxide_http_copy_bytes(NSData *data, uint8_t **out_ptr, size_t *out_len)
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

static bool oxide_http_copy_string(NSString *string, uint8_t **out_ptr, size_t *out_len)
{
   NSData *data = string == nil ? nil : [string dataUsingEncoding:NSUTF8StringEncoding];
   return oxide_http_copy_bytes(data, out_ptr, out_len);
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
   oxide_http_response_clear(out_response);
   if (url_ptr == NULL || url_len == 0 || max_response_bytes == 0)
   {
      return -1;
   }
   if ([NSThread isMainThread])
   {
      return -6;
   }

   NSString *url_string = [[NSString alloc] initWithBytes:url_ptr length:url_len encoding:NSUTF8StringEncoding];
   NSURL *url = url_string == nil ? nil : [NSURL URLWithString:url_string];
   NSString *scheme = url.scheme.lowercaseString;
   if (url == nil || !([scheme isEqualToString:@"https"] || [scheme isEqualToString:@"http"]))
   {
      return -1;
   }

   NSTimeInterval timeout = timeout_ms == 0 ? 10.0 : ((NSTimeInterval)timeout_ms / 1000.0);
   NSURLSessionConfiguration *configuration = [NSURLSessionConfiguration ephemeralSessionConfiguration];
   configuration.timeoutIntervalForRequest = timeout;
   configuration.timeoutIntervalForResource = timeout;
   configuration.waitsForConnectivity = YES;
   configuration.requestCachePolicy = NSURLRequestReloadIgnoringLocalCacheData;
   configuration.URLCache = nil;
#if TARGET_OS_IPHONE
   configuration.allowsCellularAccess = YES;
   configuration.multipathServiceType = NSURLSessionMultipathServiceTypeHandover;
#endif

   NSMutableURLRequest *request = [NSMutableURLRequest requestWithURL:url
                                                          cachePolicy:NSURLRequestReloadIgnoringLocalCacheData
                                                      timeoutInterval:timeout];
   request.HTTPMethod = @"GET";

   dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
   __block NSData *body_data = nil;
   __block NSURLResponse *url_response = nil;
   __block NSError *request_error = nil;

   NSURLSession *session = [NSURLSession sessionWithConfiguration:configuration];
   NSURLSessionDataTask *task = [session dataTaskWithRequest:request
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
   if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, wait_ns)) != 0)
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
   if (!oxide_http_copy_bytes(body_data, &out_response->body_ptr, &out_response->body_len) ||
       !oxide_http_copy_string(http_response.URL.absoluteString,
                               &out_response->final_url_ptr,
                               &out_response->final_url_len) ||
       !oxide_http_copy_string(content_type, &out_response->content_type_ptr, &out_response->content_type_len))
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
   oxide_http_response_clear(response);
}
