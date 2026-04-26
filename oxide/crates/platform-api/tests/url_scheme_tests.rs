use oxide_platform_api::url_scheme::{UrlComponents, UrlSchemeSecurity};
use std::collections::BTreeMap;

#[test]
fn parse_simple_url() {
   let url = UrlComponents::parse("nametag://profile/user123").unwrap();
   assert_eq!(url.scheme, "nametag");
   assert_eq!(url.host, Some(String::from("profile")));
   assert_eq!(url.path, Some(String::from("/user123")));
   assert!(url.query.is_empty());
}

#[test]
fn parse_url_with_query() {
   let url = UrlComponents::parse("https://example.com/path?key=value&foo=bar").unwrap();
   assert_eq!(url.scheme, "https");
   assert_eq!(url.host, Some(String::from("example.com")));
   assert_eq!(url.path, Some(String::from("/path")));
   assert_eq!(url.query.get("key"), Some(&String::from("value")));
   assert_eq!(url.query.get("foo"), Some(&String::from("bar")));
}

#[test]
fn parse_social_url() {
   let url = UrlComponents::parse("fb://profile/1228210410623166").unwrap();
   assert_eq!(url.scheme, "fb");
   assert_eq!(url.host, Some(String::from("profile")));
   assert_eq!(url.path, Some(String::from("/1228210410623166")));
}

#[test]
fn build_url() {
   let mut query = BTreeMap::new();
   query.insert(String::from("id"), String::from("123"));

   let components = UrlComponents {
      scheme: String::from("myapp"),
      host: Some(String::from("action")),
      path: Some(String::from("/perform")),
      query,
   };

   let url = components.to_url();
   assert_eq!(url, "myapp://action/perform?id=123");
}

#[test]
fn security_blocks_unsafe_schemes() {
   let security = UrlSchemeSecurity::default();
   assert!(security.is_allowed("javascript:alert('xss')").is_err());
   assert!(security.is_allowed("file:///etc/passwd").is_err());
   assert!(security.is_allowed("http://example.com").is_err());
}

#[test]
fn security_allows_safe_schemes() {
   let security = UrlSchemeSecurity::default();
   assert!(security.is_allowed("https://example.com").is_ok());
   assert!(security.is_allowed("mailto:user@example.com").is_ok());
   assert!(security.is_allowed("tel:+1234567890").is_ok());
   assert!(security.is_allowed("sms:+1234567890").is_ok());
}

#[test]
fn security_custom_allowlist() {
   let mut security = UrlSchemeSecurity::default();
   security.allowed_schemes.push(String::from("myapp"));
   security.allow_custom = true;
   assert!(security.is_allowed("myapp://action").is_ok());
}
