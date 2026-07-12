fn main()
{
   let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
   if target_os != "ios" && target_os != "macos"
   {
      return;
   }

   println!("cargo:rerun-if-changed=src/apple/http.m");
   cc::Build::new()
      .file("src/apple/http.m")
      .flag("-fobjc-arc")
      .flag("-fmodules")
      .compile("oxide_platform_apple_http");
   println!("cargo:rustc-link-lib=framework=Foundation");
   println!("cargo:rustc-link-lib=objc");
}
