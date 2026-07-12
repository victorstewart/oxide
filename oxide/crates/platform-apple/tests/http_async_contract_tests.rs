#[test]
fn apple_http_is_shared_streaming_manual_and_credential_free()
{
   let source = include_str!("../src/apple/http.m");
   assert!(source.contains("@interface OxideHttpDelegate"));
   assert!(source.contains("static OxideHttpDelegate *delegate"));
   assert!(source.contains("didReceiveData:(NSData *)data"));
   assert!(source.contains("completionHandler(nil)"));
   assert!(source.contains("configuration.HTTPCookieStorage = nil"));
   assert!(source.contains("configuration.URLCredentialStorage = nil"));
   assert!(source.contains("url.host.length == 0"));
   assert!(source.contains("configuration.URLCache = nil"));
   assert!(source.contains("response.expectedContentLength"));
   assert!(source.contains("data.length > state.maximumBytes - state.receivedBytes"));
   assert!(!source.contains("dispatch_semaphore"));
   assert!(!source.contains("dataTaskWithRequest:request\n                                           completionHandler"));
}

#[test]
fn apple_http_serializes_cancel_timeout_and_delegate_callbacks()
{
   let source = include_str!("../src/apple/http.m");
   assert!(source.contains("queue.maxConcurrentOperationCount = 1"));
   assert!(source.contains("[delegate.delegateQueue addOperationWithBlock:"));
   assert!(source.contains("takeStateForRequest"));
   assert!(source.contains("takeStateForTask"));
   assert!(source.contains("OxideHttpEventCancelled"));
   assert!(source.contains("OxideHttpEventComplete"));
}
