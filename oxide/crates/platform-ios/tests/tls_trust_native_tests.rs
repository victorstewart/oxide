#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn native_security_framework_enforces_tls_trust_policy()
{
   let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
   let source = manifest.join("tests/native/tls_trust.m");
   let binary = std::env::temp_dir().join(format!(
      "oxide-platform-ios-tls-trust-{}",
      std::process::id(),
   ));
   let output = Command::new("xcrun")
      .args(["--sdk", "macosx", "clang", "-fobjc-arc", "-fblocks", "-fmodules", "-fcxx-modules"])
      .arg(&source)
      .args(["-framework", "Foundation", "-framework", "Security", "-framework", "Network"])
      .arg("-o")
      .arg(&binary)
      .output()
      .expect("compile native TLS trust test");
   assert!(output.status.success(), "native TLS test compile failed: {}", String::from_utf8_lossy(&output.stderr));

   let output = Command::new(&binary).output().expect("run native TLS trust test");
   let _ = std::fs::remove_file(&binary);
   assert!(output.status.success(), "native TLS trust test failed: stdout={} stderr={}",
      String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
   assert!(String::from_utf8_lossy(&output.stdout).contains("native TLS trust checks passed"));
}
