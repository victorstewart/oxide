fn main()
{
   let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
   if target_os != "ios"
   {
      return;
   }

   let legacy_host =
      std::env::var_os("CARGO_FEATURE_TEST_SCENES_ENTRYPOINT").is_some();
   let app_source = if legacy_host
   {
      "src/ios/app.m"
   }
   else
   {
      "src/ios/product_app.m"
   };
   println!("cargo:rerun-if-changed=src/ios/app.m");
   println!("cargo:rerun-if-changed=src/ios/product_app.m");
   let mut build = cc::Build::new();
   build
      .file(app_source)
      .flag("-fobjc-arc")
      .flag("-fmodules")
      .flag("-fcxx-modules");
   build.define("OXIDE_HOST_USE_PLATFORM_CAMERA", Some("1"));
   if legacy_host && std::env::var_os("CARGO_FEATURE_PERF_HOST_STUBS").is_some()
   {
      println!("cargo:rerun-if-changed=src/ios/perf_stubs.m");
      build.file("src/ios/perf_stubs.m");
   }
   if std::env::var_os("CARGO_FEATURE_IOS_EDR").is_some()
   {
      build.define("EDR_ENABLED", Some("1"));
   }
   if let Ok(compiler) = std::env::var("CC")
   {
      build.compiler(compiler);
   }
   build.compile("oxide_host_ios_app");
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
   ]
   {
      println!("cargo:rustc-link-lib=framework={framework}");
   }
   println!("cargo:rustc-link-lib=objc");
}
