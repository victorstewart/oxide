fn main() {
    // Only compile and link Objective-C shim when targeting iOS.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "ios" {
        return;
    }

    println!("cargo:rerun-if-changed=src/ios/app.m");
    let mut b = cc::Build::new();
    b.file("src/ios/app.m").flag("-fobjc-arc").flag("-fmodules").flag("-fcxx-modules");
    b.define("OXIDE_HOST_USE_PLATFORM_CAMERA", Some("1"));
    if std::env::var_os("CARGO_FEATURE_PERF_HOST_STUBS").is_some() {
        println!("cargo:rerun-if-changed=src/ios/perf_stubs.m");
        b.file("src/ios/perf_stubs.m");
    }
    if std::env::var_os("CARGO_FEATURE_IOS_EDR").is_some() {
        b.define("EDR_ENABLED", Some("1"));
    }
    // Compile as Objective-C
    if let Ok(cc_path) = std::env::var("CC") {
        b.compiler(cc_path);
    }
    b.compile("oxide_host_ios_app");
    // Link required frameworks
    for framework in [
        "UIKit",
        "Foundation",
        "QuartzCore",
        "Metal",
        "CoreGraphics",
        "UserNotifications",
        "CoreLocation",
        "AVFoundation",
        "Contacts",
        "CoreBluetooth",
        "CoreMotion",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=objc");
}
