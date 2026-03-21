fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "ios" {
        return;
    }

    let mut build = cc::Build::new();
    build
        .flag("-fobjc-arc")
        .flag("-fmodules")
        .flag("-fcxx-modules")
        .file("src/ios/bluetooth.m")
        .file("src/ios/host_services.m")
        .file("src/ios/network.m")
        .file("src/ios/push.m");

    println!("cargo:rerun-if-changed=src/ios/bluetooth.m");
    println!("cargo:rerun-if-changed=src/ios/host_services.m");
    println!("cargo:rerun-if-changed=src/ios/network.m");
    println!("cargo:rerun-if-changed=src/ios/push.m");

    if std::env::var_os("CARGO_FEATURE_NATIVE_CAMERA_BRIDGE").is_some() {
        println!("cargo:rerun-if-changed=src/ios/camera.m");
        build.file("src/ios/camera.m");
    }

    build.compile("oxide_platform_ios_native");

    for framework in
        [
            "AVFoundation",
            "Contacts",
            "CoreBluetooth",
            "CoreLocation",
            "CoreMedia",
            "CoreMotion",
            "CoreVideo",
            "Foundation",
            "Metal",
            "Network",
            "Photos",
            "QuartzCore",
            "Security",
            "UIKit",
            "UserNotifications",
        ]
    {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=objc");
}
