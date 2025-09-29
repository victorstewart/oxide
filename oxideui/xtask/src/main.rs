use anyhow::{bail, Context, Result};
use plist::{Dictionary, Value as PlValue};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct CapabilitiesToml {
    #[serde(default)]
    usage_strings: BTreeMap<String, String>,
    #[serde(default)]
    entitlements: Entitlements,
}

#[derive(Debug, Default, Deserialize)]
struct Entitlements {
    #[serde(default)]
    push_notifications: bool,
    #[serde(default)]
    bluetooth_central: bool,
    #[serde(default)]
    bluetooth_peripheral: bool,
    #[serde(default)]
    background_fetch: bool,
    #[serde(default)]
    background_remote_notification: bool,
    #[serde(default)]
    background_processing: bool,
    #[serde(default)]
    location: LocationMode,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LocationMode {
    None,
    WhenInUse,
    Always,
}
impl Default for LocationMode {
    fn default() -> Self {
        LocationMode::None
    }
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match (args.get(0).map(String::as_str), args.get(1).map(String::as_str)) {
        (Some("ios"), Some("prepare")) => ios_prepare(),
        (Some("test-all"), _) => test_all(),
        _ => {
            eprintln!("Usage:\n  cargo xtask ios prepare\n  cargo xtask test-all");
            Ok(())
        }
    }
}

fn ios_prepare() -> Result<()> {
    let root = locate_workspace_root()?;
    let app_dir = root.join("host/ios-app/App");
    let caps_toml = app_dir.join("capabilities.toml");
    let info_plist = app_dir.join("Info.plist");
    let entitlements_plist = app_dir.join("App.entitlements");

    let caps: CapabilitiesToml = {
        let text = fs::read_to_string(&caps_toml)
            .with_context(|| format!("reading {}", caps_toml.display()))?;
        toml::from_str(&text).with_context(|| "parsing capabilities.toml")?
    };

    validate_usage(&caps)?;

    // Generate entitlements
    let ent = build_entitlements_dict(&caps.entitlements);
    let ent_plist = PlValue::Dictionary(ent);
    plist::to_file_xml(&entitlements_plist, &ent_plist)
        .with_context(|| "writing App.entitlements")?;

    // Merge Info.plist
    let mut info = read_plist_dict(&info_plist).unwrap_or_else(Dictionary::new);
    merge_usage_strings(&mut info, &caps.usage_strings);
    merge_background_modes(&mut info, &caps.entitlements);
    plist::to_file_xml(&info_plist, &PlValue::Dictionary(info))
        .with_context(|| "writing Info.plist")?;

    // Build and bundle shaders (default.metallib)
    build_and_bundle_shaders(&root, &app_dir)?;

    println!("Prepared entitlements, Info.plist, and bundled shaders.");
    Ok(())
}

fn test_all() -> Result<()> {
    let root = locate_workspace_root()?;

    run_fmt_check(&root)?;
    run_command(
        &root,
        "cargo",
        &["clippy", "--workspace", "--all-targets", "--all-features", "-D", "warnings"],
        false,
    )?;
    run_command(
        &root,
        "cargo",
        &["test", "--workspace", "--all-targets", "--all-features", "--quiet"],
        false,
    )?;
    run_command(
        &root,
        "cargo",
        &["test", "--workspace", "--no-default-features", "--quiet"],
        false,
    )?;
    run_command(&root, "cargo", &["hack", "check", "--each-feature", "--no-dev-deps"], true)?;

    Ok(())
}

fn run_fmt_check(root: &Path) -> Result<()> {
    println!("> cargo fmt --all --check");
    let output = Command::new("cargo")
        .arg("fmt")
        .arg("--all")
        .arg("--check")
        .current_dir(root)
        .output()
        .with_context(|| "running cargo fmt")?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("nightly") && stderr.contains("rustfmt") {
        println!("cargo fmt skipped: nightly rustfmt unavailable\n{}", stderr.trim());
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        eprintln!("{}", stdout.trim());
    }
    if !stderr.trim().is_empty() {
        eprintln!("{}", stderr.trim());
    }
    bail!("cargo fmt --all --check failed")
}

fn run_command(root: &Path, program: &str, args: &[&str], allow_fail: bool) -> Result<()> {
    println!("> {} {}", program, args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) => {
            if allow_fail && e.kind() == ErrorKind::NotFound {
                println!("{} not found (skipping)", program);
                return Ok(());
            }
            return Err(e).with_context(|| format!("running {} {}", program, args.join(" ")));
        }
    };
    if status.success() {
        return Ok(());
    }
    if allow_fail {
        println!("{} {} failed (non-fatal)", program, args.join(" "));
        return Ok(());
    }
    bail!("{} {} failed with status {}", program, args.join(" "), status.code().unwrap_or(-1))
}

fn locate_workspace_root() -> Result<PathBuf> {
    // xtask is at <root>/xtask. Walk up until we find Cargo.toml containing [workspace]
    let mut p = std::env::current_dir()?;
    for _ in 0..5 {
        let ct = p.join("Cargo.toml");
        if ct.exists() {
            let s = fs::read_to_string(&ct)?;
            if s.contains("[workspace]") {
                return Ok(p);
            }
        }
        if !p.pop() {
            break;
        }
    }
    bail!("workspace root not found")
}

fn read_plist_dict(path: &Path) -> Option<Dictionary> {
    let v: PlValue = plist::from_file(path).ok()?;
    match v {
        PlValue::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn merge_usage_strings(info: &mut Dictionary, usage: &BTreeMap<String, String>) {
    for (k, v) in usage {
        info.insert(k.clone(), PlValue::String(v.clone()));
    }
}

fn merge_background_modes(info: &mut Dictionary, ent: &Entitlements) {
    let mut modes: Vec<String> = Vec::new();
    if ent.background_remote_notification {
        modes.push("remote-notification".into());
    }
    if ent.background_fetch {
        modes.push("fetch".into());
    }
    if ent.background_processing {
        modes.push("processing".into());
    }
    if ent.bluetooth_central {
        modes.push("bluetooth-central".into());
    }
    if ent.bluetooth_peripheral {
        modes.push("bluetooth-peripheral".into());
    }
    if !modes.is_empty() {
        let arr = PlValue::Array(modes.into_iter().map(PlValue::String).collect());
        info.insert("UIBackgroundModes".into(), arr);
    }
}

fn build_entitlements_dict(e: &Entitlements) -> Dictionary {
    let mut d = Dictionary::new();
    if e.push_notifications {
        d.insert("aps-environment".into(), PlValue::String("development".into()));
    }
    // Spec requests Bluetooth roles under entitlements (engine will gate APIs regardless)
    if e.bluetooth_central || e.bluetooth_peripheral {
        let mut roles: Vec<PlValue> = Vec::new();
        if e.bluetooth_central {
            roles.push(PlValue::String("central".into()));
        }
        if e.bluetooth_peripheral {
            roles.push(PlValue::String("peripheral".into()));
        }
        d.insert("com.apple.developer.bluetooth".into(), PlValue::Array(roles));
    }
    d
}

fn validate_usage(c: &CapabilitiesToml) -> Result<()> {
    let u = &c.usage_strings;
    // Required keys for chosen capabilities
    if c.entitlements.bluetooth_central && !u.contains_key("NSBluetoothAlwaysUsageDescription") {
        bail!("Missing NSBluetoothAlwaysUsageDescription for bluetooth_central=true");
    }
    if c.entitlements.bluetooth_peripheral
        && !u.contains_key("NSBluetoothPeripheralUsageDescription")
    {
        bail!("Missing NSBluetoothPeripheralUsageDescription for bluetooth_peripheral=true");
    }
    match c.entitlements.location {
        LocationMode::None => {}
        LocationMode::WhenInUse => {
            if !u.contains_key("NSLocationWhenInUseUsageDescription") {
                bail!("Missing NSLocationWhenInUseUsageDescription for location=when_in_use");
            }
        }
        LocationMode::Always => {
            if !u.contains_key("NSLocationAlwaysAndWhenInUseUsageDescription") {
                bail!("Missing NSLocationAlwaysAndWhenInUseUsageDescription for location=always");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entitlements_gen() {
        let e = Entitlements {
            push_notifications: true,
            bluetooth_central: true,
            bluetooth_peripheral: false,
            background_fetch: true,
            background_remote_notification: true,
            background_processing: false,
            location: LocationMode::WhenInUse,
        };
        let d = build_entitlements_dict(&e);
        assert_eq!(d.get("aps-environment").and_then(|v| v.as_string()), Some("development"));
        let info = &mut Dictionary::new();
        let mut usage = BTreeMap::new();
        usage.insert("NSBluetoothAlwaysUsageDescription".into(), "Needed".into());
        usage.insert("NSLocationWhenInUseUsageDescription".into(), "Needed".into());
        merge_usage_strings(info, &usage);
        merge_background_modes(info, &e);
        assert!(info.contains_key("UIBackgroundModes"));
    }
}

fn build_and_bundle_shaders(root: &Path, app_dir: &Path) -> Result<()> {
    let shaders = root.join("crates/renderer-metal/shaders");
    if !shaders.exists() {
        return Ok(());
    }
    // Ensure resources dir
    let res_dir = app_dir.join("Resources");
    fs::create_dir_all(&res_dir).with_context(|| format!("creating {}", res_dir.display()))?;

    // Determine SDK
    let target = std::env::var("TARGET").unwrap_or_default();
    let sdk = if target.contains("apple-ios") {
        if target.contains("sim") {
            "iphonesimulator"
        } else {
            "iphoneos"
        }
    } else {
        "iphoneos"
    };

    // Compile all .metal to .air
    let mut airs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&shaders).with_context(|| "reading shaders dir")? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("metal") {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let air = res_dir.join(format!("{stem}.air"));
            let status = std::process::Command::new("xcrun")
                .args(["-sdk", sdk, "metal", "-c"])
                .arg(&p)
                .args(["-o"])
                .arg(&air)
                .status()?;
            if !status.success() {
                bail!("metal compile failed for {}", p.display());
            }
            airs.push(air);
        }
    }
    if airs.is_empty() {
        return Ok(());
    }
    // Link metallib
    let metallib = res_dir.join("default.metallib");
    let mut cmd = std::process::Command::new("xcrun");
    cmd.args(["-sdk", sdk, "metallib"]).args(airs.iter().map(|p| p.as_os_str()));
    cmd.arg("-o").arg(&metallib);
    let status = cmd.status()?;
    if !status.success() {
        bail!("metallib link failed");
    }
    // Cleanup .air files
    for a in airs {
        let _ = fs::remove_file(a);
    }
    Ok(())
}
