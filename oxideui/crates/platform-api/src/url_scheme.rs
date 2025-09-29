//! URL scheme handling for deep linking and inter-app communication

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

/// Parsed URL components
#[derive(Clone, Debug)]
pub struct UrlComponents {
    pub scheme: String,        // "nametag", "https", "fb", etc.
    pub host: Option<String>,  // "example.com", "profile", etc.
    pub path: Option<String>,  // "/user/123"
    pub query: BTreeMap<String, String>, // ?key=value&foo=bar
}

impl UrlComponents {
    /// Parse a URL string into components
    pub fn parse(url: &str) -> Option<Self> {
        // Find scheme
        let scheme_end = url.find("://")?;
        let scheme = url[..scheme_end].to_string();
        let rest = &url[scheme_end + 3..];

        // Split host/path/query
        let (host_path, query_str) = if let Some(q_idx) = rest.find('?') {
            (&rest[..q_idx], Some(&rest[q_idx + 1..]))
        } else {
            (rest, None)
        };

        let (host, path) = if let Some(slash_idx) = host_path.find('/') {
            (
                Some(host_path[..slash_idx].to_string()),
                Some(host_path[slash_idx..].to_string()),
            )
        } else if !host_path.is_empty() {
            (Some(host_path.to_string()), None)
        } else {
            (None, None)
        };

        // Parse query params
        let mut query = BTreeMap::new();
        if let Some(q) = query_str {
            for pair in q.split('&') {
                if let Some(eq_idx) = pair.find('=') {
                    let key = pair[..eq_idx].to_string();
                    let value = pair[eq_idx + 1..].to_string();
                    query.insert(key, value);
                }
            }
        }

        Some(Self { scheme, host, path, query })
    }

    /// Build URL string from components
    pub fn to_url(&self) -> String {
        let mut url = alloc::format!("{}://", self.scheme);

        if let Some(h) = &self.host {
            url.push_str(h);
        }

        if let Some(p) = &self.path {
            url.push_str(p);
        }

        if !self.query.is_empty() {
            url.push('?');
            let mut first = true;
            for (k, v) in &self.query {
                if !first {
                    url.push('&');
                }
                url.push_str(k);
                url.push('=');
                url.push_str(v);
                first = false;
            }
        }

        url
    }
}

/// URL open result
#[derive(Clone, Debug, PartialEq)]
pub enum UrlOpenResult {
    Opened,
    NotSupported,
    Blocked(String),  // Security blocked
    Error(String),
}

/// Security configuration for URL schemes
#[derive(Clone, Debug)]
pub struct UrlSchemeSecurity {
    /// Allowed URL schemes (whitelist)
    pub allowed_schemes: Vec<String>,
    /// Blocked URL schemes (blacklist)
    pub blocked_schemes: Vec<String>,
    /// Allow http/https schemes
    pub allow_http: bool,
    /// Allow custom app schemes
    pub allow_custom: bool,
}

impl Default for UrlSchemeSecurity {
    fn default() -> Self {
        Self {
            // Safe defaults: only allow https and common social schemes
            allowed_schemes: alloc::vec![
                String::from("https"),
                String::from("mailto"),
                String::from("tel"),
                String::from("sms"),
            ],
            blocked_schemes: alloc::vec![
                String::from("file"),
                String::from("javascript"),
                String::from("data"),
                String::from("about"),
            ],
            allow_http: false,
            allow_custom: false,
        }
    }
}

impl UrlSchemeSecurity {
    /// Check if a URL is allowed based on security rules
    pub fn is_allowed(&self, url: &str) -> Result<(), String> {
        let components = UrlComponents::parse(url)
            .ok_or_else(|| String::from("Invalid URL format"))?;

        // Check blacklist first
        if self.blocked_schemes.contains(&components.scheme) {
            return Err(alloc::format!("Scheme '{}' is blocked", components.scheme));
        }

        // Check if http is allowed
        if components.scheme == "http" && !self.allow_http {
            return Err(String::from("HTTP URLs are not allowed (use HTTPS)"));
        }

        // Check whitelist
        if !self.allowed_schemes.is_empty() {
            if !self.allowed_schemes.contains(&components.scheme) {
                if !self.allow_custom {
                    return Err(alloc::format!("Scheme '{}' is not in allowlist", components.scheme));
                }
            }
        }

        // Additional security checks
        if components.scheme == "javascript" {
            return Err(String::from("JavaScript URLs are forbidden"));
        }

        if components.scheme == "file" {
            return Err(String::from("File URLs are forbidden"));
        }

        Ok(())
    }
}

/// URL scheme handler trait
pub trait UrlSchemeHandler: Send + Sync {
    /// Get current security configuration
    fn security(&self) -> &UrlSchemeSecurity;

    /// Set security configuration
    fn set_security(&mut self, security: UrlSchemeSecurity);

    /// Check if URL can be opened (app is installed)
    fn can_open(&self, url: &str) -> bool;

    /// Open URL with security validation (launches external app or handles internally)
    fn open(&mut self, url: &str) -> UrlOpenResult {
        // Validate against security rules first
        if let Err(reason) = self.security().is_allowed(url) {
            return UrlOpenResult::Blocked(reason);
        }

        // Proceed with actual opening
        self.open_unchecked(url)
    }

    /// Open URL without security checks (for internal use)
    fn open_unchecked(&mut self, url: &str) -> UrlOpenResult;

    /// Register app's custom URL scheme handler
    /// Callback receives URLs when app is opened via custom scheme
    fn register_handler<F>(&mut self, callback: F)
    where
        F: Fn(UrlComponents) + Send + 'static;
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn security_blocks_javascript() {
        let security = UrlSchemeSecurity::default();
        assert!(security.is_allowed("javascript:alert('xss')").is_err());
    }

    #[test]
    fn security_blocks_file() {
        let security = UrlSchemeSecurity::default();
        assert!(security.is_allowed("file:///etc/passwd").is_err());
    }

    #[test]
    fn security_blocks_http_by_default() {
        let security = UrlSchemeSecurity::default();
        assert!(security.is_allowed("http://example.com").is_err());
    }

    #[test]
    fn security_allows_https() {
        let security = UrlSchemeSecurity::default();
        assert!(security.is_allowed("https://example.com").is_ok());
    }

    #[test]
    fn security_allows_safe_schemes() {
        let security = UrlSchemeSecurity::default();
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
}