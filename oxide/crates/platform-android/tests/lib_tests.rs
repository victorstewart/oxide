use oxide_platform_android::AndroidProductionHttpRequired;

#[test]
fn non_android_workspace_exposes_shipping_gate_marker()
{
   assert_eq!(AndroidProductionHttpRequired, AndroidProductionHttpRequired);
}

#[test]
fn android_shipping_fails_at_compile_time_until_a_real_http_host_exists()
{
   let source = include_str!("../src/lib.rs");
   assert!(source.contains("#[cfg(target_os = \"android\")]\ncompile_error!(\"Oxide Android shipping is disabled:"));
   assert!(!source.contains("UnsupportedHttpClient"));
}
