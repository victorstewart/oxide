const HOST_SERVICES: &str = include_str!(concat!(
   env!("CARGO_MANIFEST_DIR"),
   "/src/ios/host_services.m"
));

#[test]
fn external_urls_require_a_main_thread_handler_check_before_opening()
{
   let implementation = HOST_SERVICES
      .split("int32_t oxide_host_open_external_url")
      .nth(1)
      .expect("external URL implementation")
      .split("uint32_t oxide_host_max_framerate_hz")
      .next()
      .expect("external URL implementation end");
   let main = implementation.find("dispatch_main_sync").expect("main-thread dispatch");
   let can_open = implementation.find("[application canOpenURL:url]").expect("handler check");
   let open = implementation.find("[application openURL:url").expect("URL open");
   assert!(main < can_open && can_open < open);
}

#[test]
fn standard_paths_create_the_exact_returned_oxide_directory()
{
   let implementation = HOST_SERVICES
      .split("int32_t oxide_host_standard_path")
      .nth(1)
      .expect("standard path implementation")
      .split("static UIImpactFeedbackGenerator *")
      .next()
      .expect("standard path implementation end");
   let append = implementation
      .find("URLByAppendingPathComponent:@\"Oxide\" isDirectory:YES")
      .expect("Oxide directory append");
   let create = implementation.find("createDirectoryAtURL:url").expect("directory creation");
   assert!(append < create);
}

#[test]
fn optional_nametag_symbols_are_compile_time_guarded()
{
   let declaration = HOST_SERVICES.find("extern void nametag_host_update_permission").expect("Nametag declaration");
   let guard = HOST_SERVICES[..declaration]
      .rfind("#ifndef OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE")
      .expect("Nametag bridge guard");
   let close = HOST_SERVICES[declaration..].find("#endif").expect("Nametag bridge guard end");
   assert!(guard < declaration && close > 0);
}
