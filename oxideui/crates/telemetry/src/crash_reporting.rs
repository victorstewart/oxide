//! Crash reporting and error tracking extensions for telemetry

use std::string::String;
use std::vec::Vec;
use std::collections::BTreeMap;

/// Crash report metadata
#[derive(Clone, Debug)]
pub struct CrashReport {
    pub timestamp: u64,
    pub version: String,
    pub build: String,
    pub platform: String,
    pub os_version: String,
    pub device_model: String,
    pub thread_info: Vec<ThreadInfo>,
    pub exception_type: Option<String>,
    pub exception_message: Option<String>,
    pub custom_data: BTreeMap<String, String>,
}

/// Thread backtrace information
#[derive(Clone, Debug)]
pub struct ThreadInfo {
    pub thread_id: u64,
    pub crashed: bool,
    pub frames: Vec<StackFrame>,
}

/// Stack frame in backtrace
#[derive(Clone, Debug)]
pub struct StackFrame {
    pub address: u64,
    pub symbol: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

/// Breadcrumb for debugging crashes
#[derive(Clone, Debug)]
pub struct Breadcrumb {
    pub timestamp: u64,
    pub category: String,
    pub message: String,
    pub level: BreadcrumbLevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreadcrumbLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// Crash reporting trait (integrate with Crashlytics, Sentry, etc.)
pub trait CrashReporter {
    /// Initialize crash reporter
    fn initialize(&mut self, api_key: &str);

    /// Set user identifier for crash reports
    fn set_user_id(&mut self, user_id: &str);

    /// Add custom key-value data to crash reports
    fn set_custom_value(&mut self, key: &str, value: &str);

    /// Log breadcrumb for crash debugging
    fn log_breadcrumb(&mut self, breadcrumb: Breadcrumb);

    /// Record non-fatal error
    fn record_error(&mut self, error: &str, context: BTreeMap<String, String>);

    /// Force send pending reports
    fn send_pending_reports(&mut self);

    /// Check if crash reporter is enabled
    fn is_enabled(&self) -> bool;
}

/// No-op crash reporter for development/testing
#[derive(Default)]
pub struct NullCrashReporter;

impl CrashReporter for NullCrashReporter {
    fn initialize(&mut self, _api_key: &str) {}
    fn set_user_id(&mut self, _user_id: &str) {}
    fn set_custom_value(&mut self, _key: &str, _value: &str) {}
    fn log_breadcrumb(&mut self, _breadcrumb: Breadcrumb) {}
    fn record_error(&mut self, _error: &str, _context: BTreeMap<String, String>) {}
    fn send_pending_reports(&mut self) {}
    fn is_enabled(&self) -> bool { false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_reporter_disabled() {
        let reporter = NullCrashReporter::default();
        assert!(!reporter.is_enabled());
    }

    #[test]
    fn breadcrumb_creation() {
        let breadcrumb = Breadcrumb {
            timestamp: 1234567890,
            category: String::from("navigation"),
            message: String::from("User navigated to profile"),
            level: BreadcrumbLevel::Info,
        };
        assert_eq!(breadcrumb.category, "navigation");
    }
}