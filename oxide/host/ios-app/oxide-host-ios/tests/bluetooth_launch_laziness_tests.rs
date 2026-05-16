use std::path::Path;

fn host_source() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("read host source")
}

fn source_between<'a>(source: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
    let start = source.find(start_marker).expect(start_marker);
    let end = source[start..].find(end_marker).expect(end_marker) + start;
    &source[start..end]
}

#[test]
fn ios_bluetooth_runtime_is_opt_in_by_default() {
    let source = host_source();
    let body = source_between(&source, "fn bluetooth_runtime_enabled() -> bool", "#[no_mangle]");

    assert!(
        body.contains("OXIDE_ENABLE_BLUETOOTH") && body.trim_end().ends_with("false\n}"),
        "OxideHost must not create Bluetooth runtime objects unless explicitly enabled"
    );
}

#[test]
fn startup_permission_snapshot_skips_bluetooth_status() {
    let source = host_source();
    let domains = source_between(&source, "const PERMISSION_DOMAINS:", "fn initialize_permissions");

    assert!(
        !domains.contains("PermissionDomain::Bluetooth"),
        "OxideHost startup permission snapshots must not query Bluetooth"
    );
}
