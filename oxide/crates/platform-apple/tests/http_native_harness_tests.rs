#![cfg(target_os = "macos")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn native_http_limits_hold_through_local_url_protocol()
{
   let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
   let output_dir = std::env::temp_dir().join(format!("oxide-http-native-{}", std::process::id()));
   fs::create_dir_all(&output_dir).expect("native HTTP harness output directory");
   let executable = output_dir.join("http_native_tests");
   let compile = Command::new("xcrun")
      .args(["clang", "-DOXIDE_HTTP_TESTING=1", "-fobjc-arc", "-Wall", "-Wextra", "-Werror", "-framework", "Foundation"])
      .arg(manifest.join("src/apple/http.m"))
      .arg(manifest.join("tests/http_native_tests.m"))
      .arg("-o")
      .arg(&executable)
      .output();
   let run = compile.as_ref().ok().filter(|output| output.status.success())
      .map(|_| Command::new(&executable).output());
   let cleanup = fs::remove_dir_all(output_dir);
   let compile = compile.expect("compile native HTTP harness");
   cleanup.expect("remove native HTTP harness output");
   assert!(compile.status.success(), "native HTTP harness compile failed:\n{}", String::from_utf8_lossy(&compile.stderr));
   let run = run.expect("native HTTP harness compile status").expect("run native HTTP harness");
   assert!(run.status.success(), "native HTTP harness failed:\n{}", String::from_utf8_lossy(&run.stderr));
}
