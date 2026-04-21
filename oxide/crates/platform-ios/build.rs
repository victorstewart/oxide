fn add_objc_source(build: &mut cc::Build, path: &'static str) {
    println!("cargo:rerun-if-changed={path}");
    build.file(path);
}

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "ios" {
        return;
    }

    let nametag_host_bridge = std::env::var_os("CARGO_FEATURE_NAMETAG_HOST_BRIDGE").is_some();
    let mut build = cc::Build::new();
    build.flag("-fobjc-arc").flag("-fmodules").flag("-fcxx-modules");
    for source in [
        "src/ios/bluetooth.m",
        "src/ios/location.m",
        "src/ios/motion.m",
        "src/ios/host_services.m",
        "src/ios/network.m",
    ] {
        add_objc_source(&mut build, source);
    }

    if nametag_host_bridge {
        build.define("OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE", Some("1"));
    }
    add_objc_source(&mut build, "src/ios/push.m");

    if std::env::var_os("CARGO_FEATURE_NATIVE_CAMERA_BRIDGE").is_some() {
        add_objc_source(&mut build, "src/ios/camera.m");
    }

    build.compile("oxide_platform_ios_native");

    for framework in [
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
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=objc");
}
