fn main()
{
   if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "macos"
   {
      return;
   }

   println!("cargo:rerun-if-changed=src/app.m");
   let mut build = cc::Build::new();
   build.file("src/app.m").flag("-fobjc-arc");
   build.compile("oxide_macos_comparison_host");
   for framework in ["AppKit", "QuartzCore", "Metal", "CoreGraphics", "Foundation"]
   {
      println!("cargo:rustc-link-lib=framework={framework}");
   }
   println!("cargo:rustc-link-lib=objc");
}
