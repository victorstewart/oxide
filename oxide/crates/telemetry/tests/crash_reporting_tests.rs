use oxide_telemetry::crash_reporting::{
    Breadcrumb, BreadcrumbLevel, CrashReporter, NullCrashReporter,
};

#[test]
fn null_reporter_disabled() {
    let reporter = NullCrashReporter;
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
