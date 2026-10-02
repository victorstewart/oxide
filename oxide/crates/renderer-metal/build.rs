use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn ensure_placeholder(out_dir: &Path) -> anyhow::Result<()> {
    for name in ["default.metallib", "srgb.metallib"] {
        let placeholder = out_dir.join(name);
        if !placeholder.exists() {
            fs::write(&placeholder, &[] as &[u8])?;
        }
    }
    Ok(())
}

fn have_tool(sdk: &str, tool: &str) -> bool {
    std::process::Command::new("xcrun")
        .args(["-sdk", sdk, "-f", tool])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn main() -> anyhow::Result<()> {
    // Precompile both output policies; selecting one never compiles shaders in a frame.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let shader_dir = Path::new("shaders");
    println!("cargo:rerun-if-changed={}", shader_dir.display());

    if !shader_dir.exists() {
        if target_is_apple(&env::var("TARGET").unwrap_or_default()) {
            anyhow::bail!("Metal shader directory missing at {}", shader_dir.display());
        }
        ensure_placeholder(&out_dir)?;
        return Ok(());
    }

    let target = env::var("TARGET").unwrap_or_default();
    let sdk = if target.contains("apple-ios") {
        if target.contains("sim") {
            "iphonesimulator"
        } else {
            "iphoneos"
        }
    } else if target.contains("apple-darwin") {
        "macosx"
    } else {
        ensure_placeholder(&out_dir)?;
        return Ok(());
    };

    if !have_tool(sdk, "metal") || !have_tool(sdk, "metallib") {
        anyhow::bail!(
            "Metal toolchain not found for sdk {sdk}; cannot build renderer-metal default.metallib"
        );
    }

    for (name, srgb_compositing) in [("default", false), ("srgb", true)] {
        let mut air_files: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(shader_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("metal") {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let air = out_dir.join(format!("{name}-{stem}.air"));
                let mut cmd = std::process::Command::new("xcrun");
                cmd.args(["-sdk", sdk, "metal", "-c"]);
                if srgb_compositing {
                    cmd.arg("-DOXIDE_SRGB_COMPOSITING=1");
                }
                let status = cmd.arg(&path).args(["-o"]).arg(&air).status()?;
                if !status.success() {
                    anyhow::bail!("metal compile failed for {}", path.display());
                }
                air_files.push(air);
            }
        }
        if air_files.is_empty() {
            anyhow::bail!("no Metal shader sources found in {}", shader_dir.display());
        }
        let metallib = out_dir.join(format!("{name}.metallib"));
        let status = std::process::Command::new("xcrun")
            .args(["-sdk", sdk, "metallib"])
            .args(air_files.iter().map(|path| path.as_os_str()))
            .arg("-o")
            .arg(&metallib)
            .status()?;
        if !status.success() {
            anyhow::bail!("metallib link failed for {}", metallib.display());
        }
    }
    println!("cargo:warning=Generated default.metallib and srgb.metallib");
    Ok(())
}

fn target_is_apple(target: &str) -> bool {
    target.contains("apple-ios") || target.contains("apple-darwin")
}
