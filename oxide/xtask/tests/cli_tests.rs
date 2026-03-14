use std::fs;
use std::io::Write;
use std::path::Path;

use tempfile::tempdir;

use xtask::{build_and_bundle_shaders, run_cli};

#[test]
fn run_cli_unknown_command_shows_usage() {
    assert!(run_cli(&[]).is_ok());
    assert!(run_cli(&["unknown".into()]).is_ok());
}

fn with_stub_xcrun<F>(f: F)
where
    F: FnOnce(&Path),
{
    let temp = tempdir().expect("tempdir");
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let stub = bin.join("xcrun");
    let script = "#!/bin/bash\ncmd=\"${3:-}\"\nif [[ \"$cmd\" == \"metal\" ]]; then\n  out=\"\"\n  for ((i=1;i<=$#;i++)); do\n    arg=${!i}\n    if [[ \"$arg\" == \"-o\" ]]; then\n      j=$((i+1))\n      out=${!j}\n    fi\n  done\n  touch \"$out\"\n  exit 0\nelif [[ \"$cmd\" == \"metallib\" ]]; then\n  out=\"\"\n  for ((i=1;i<=$#;i++)); do\n    arg=${!i}\n    if [[ \"$arg\" == \"-o\" ]]; then\n      j=$((i+1))\n      out=${!j}\n    fi\n  done\n  touch \"$out\"\n  exit 0\nfi\nexit 0\n";
    fs::write(&stub, script).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).expect("chmod");
    }

    let prev_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin.display(), prev_path);
    std::env::set_var("PATH", &new_path);
    f(temp.path());
    std::env::set_var("PATH", prev_path);
}

#[test]
fn shader_bundler_skips_when_no_shaders() {
    let workspace = tempdir().expect("workspace");
    let root = workspace.path();
    let app_dir = root.join("app");
    fs::create_dir_all(&app_dir).expect("app dir");
    assert!(build_and_bundle_shaders(root, &app_dir).is_ok());
    assert!(!app_dir.join("Resources").exists());
}

#[test]
fn shader_bundler_runs_with_stub_compiler() {
    with_stub_xcrun(|root| {
        let shaders = root.join("crates/renderer-metal/shaders");
        fs::create_dir_all(&shaders).expect("shaders dir");
        let shader_path = shaders.join("demo.metal");
        fs::File::create(&shader_path)
            .and_then(|mut f| f.write_all(b"// metal shader"))
            .expect("write shader");

        let app_dir = root.join("app");
        fs::create_dir_all(&app_dir).expect("app dir");
        let prev_target = std::env::var("TARGET").ok();
        std::env::set_var("TARGET", "aarch64-apple-ios-sim");
        let result = build_and_bundle_shaders(root, &app_dir);
        if let Some(val) = prev_target {
            std::env::set_var("TARGET", val);
        } else {
            std::env::remove_var("TARGET");
        }
        assert!(result.is_ok());
        let metallib = app_dir.join("Resources/default.metallib");
        assert!(metallib.exists());
        let air_files: Vec<_> = fs::read_dir(app_dir.join("Resources"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("air"))
            .collect();
        assert!(air_files.is_empty());
    });
}
