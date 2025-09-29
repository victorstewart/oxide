fn main() {
    // Only compile and link Objective-C shim when targeting iOS.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        println!("cargo:rerun-if-changed=src/ios/app.m");
        let mut b = cc::Build::new();
        b.file("src/ios/app.m").flag("-fobjc-arc").flag("-fmodules").flag("-fcxx-modules");
        if std::env::var_os("CARGO_FEATURE_IOS_EDR").is_some() {
            b.define("EDR_ENABLED", Some("1"));
        }
        // Compile as Objective-C
        if let Ok(cc_path) = std::env::var("CC") {
            b.compiler(cc_path);
        }
        b.compile("oxideui_host_ios_app");
        // Link required frameworks
        println!("cargo:rustc-link-lib=framework=UIKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=UserNotifications");
        println!("cargo:rustc-link-lib=framework=CoreLocation");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Contacts");
        println!("cargo:rustc-link-lib=framework=CoreBluetooth");
        println!("cargo:rustc-link-lib=framework=CoreMotion");
        println!("cargo:rustc-link-lib=objc");
    }
}
