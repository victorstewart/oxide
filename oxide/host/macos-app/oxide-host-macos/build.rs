fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "macos" {
        println!("cargo:rerun-if-changed=src/macos/app.m");
        println!("cargo:rerun-if-changed=../../../crates/platform-apple/src/apple/bluetooth.m");
        println!(
            "cargo:rerun-if-changed=../../../crates/platform-apple/src/apple/secure_storage.m"
        );
        let mut b = cc::Build::new();
        b.file("src/macos/app.m")
            .file("../../../crates/platform-apple/src/apple/bluetooth.m")
            .file("../../../crates/platform-apple/src/apple/secure_storage.m")
            .flag("-fobjc-arc");
        if std::env::var_os("CARGO_FEATURE_HOST_TESTING").is_some() {
            b.define("OXIDE_HOST_TESTING", None);
        }
        b.compile("oxide_host_macos_app");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Contacts");
        println!("cargo:rustc-link-lib=framework=CoreBluetooth");
        println!("cargo:rustc-link-lib=framework=CoreLocation");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Photos");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=CoreVideo");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=Network");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=UserNotifications");
        println!("cargo:rustc-link-lib=framework=WebKit");
        println!("cargo:rustc-link-lib=objc");
    }
}
