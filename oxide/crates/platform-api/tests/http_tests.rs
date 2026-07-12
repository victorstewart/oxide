use oxide_platform_api::{HttpClient, HttpCredentials, HttpEvent, HttpMethod, HttpRequest, PlatformError, UnsupportedHttpClient};
use std::time::Duration;

#[test]
fn request_owns_remaining_timeout_headers_and_response_selection()
{
   let request = HttpRequest::get("https://oxide.test/page")
      .with_timeout(Duration::from_secs(3))
      .with_max_response_bytes(4096)
      .with_header("Accept", "text/html")
      .select_response_header("Content-Type")
      .select_response_header("Location");

   assert_eq!(request.timeout, Duration::from_secs(3));
   assert_eq!(request.max_response_bytes, 4096);
   assert_eq!(request.headers[0].name, "Accept");
   assert_eq!(request.headers[0].value, "text/html");
   assert_eq!(request.response_headers, ["Content-Type", "Location"]);
}

#[test]
fn post_body_and_ambient_credentials_are_explicit()
{
   let request = HttpRequest::post("/same-origin", b"field=value".to_vec())
      .with_credentials(HttpCredentials::SameOrigin);
   assert_eq!(request.method, HttpMethod::Post);
   assert_eq!(request.body, b"field=value");
   assert_eq!(request.credentials, HttpCredentials::SameOrigin);
   assert_eq!(HttpRequest::get("https://oxide.test").credentials, HttpCredentials::Omit);
}

#[test]
fn terminal_event_classification_and_unsupported_admission_are_exact()
{
   assert!(!HttpEvent::Body(vec![1]).terminal());
   assert!(HttpEvent::Complete.terminal());
   assert!(HttpEvent::Cancelled.terminal());
   assert!(HttpEvent::Failed(PlatformError::Busy).terminal());

   let client: &dyn HttpClient = &UnsupportedHttpClient;
   let result = client.start(HttpRequest::get("https://oxide.test"), Box::new(|_| {}));
   assert!(matches!(result, Err(PlatformError::Unsupported(_))));
}
