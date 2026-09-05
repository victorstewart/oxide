use oxide_host_ios::{install_app, oxide_host_is_injected_app, InstallAppError};
use oxide_platform_api::{App, AppEvent, InitContext, UpdateContext};

struct EmptyApp;

impl App for EmptyApp
{
   fn init(&mut self, _context: &mut InitContext)
   {
   }

   fn event(&mut self, _event: AppEvent, _context: &mut UpdateContext)
   {
   }
}

#[test]
#[cfg(not(feature = "test-scenes-entrypoint"))]
fn app_installation_is_explicit_and_single_owner()
{
   assert_eq!(oxide_host_is_injected_app(), 0);
   assert_eq!(install_app(Box::new(EmptyApp)), Ok(()));
   assert_eq!(oxide_host_is_injected_app(), 1);
   assert_eq!(install_app(Box::new(EmptyApp)), Err(InstallAppError::AlreadyInstalled));
}

#[test]
#[cfg(feature = "test-scenes-entrypoint")]
fn legacy_test_host_rejects_production_app_installation()
{
   assert_eq!(
      install_app(Box::new(EmptyApp)),
      Err(InstallAppError::LegacyTestHostSelected),
   );
   assert_eq!(oxide_host_is_injected_app(), 0);
}

#[test]
fn runtime_image_upload_maps_invalid_handles_and_releases_owned_resources()
{
   let source = include_str!("../src/lib.rs");
   let start = source.find("impl gfx_api::RuntimeImageUploader for MtlUploader").expect("uploader");
   let tail = &source[start..];
   let end = tail.find("struct LegacyDrawEncoder").expect("uploader end");
   let uploader = &tail[..end];

   assert!(uploader.contains("fn try_create_rgba8_sampled("));
   assert!(uploader.contains(
      "(*self.renderer).image_create_rgba8_sampled(w, h, data, row_bytes, sampling)",
   ));
   assert_eq!(
      uploader.matches("(handle.0 != 0).then_some(handle)").count(),
      2,
      "both RGBA create paths must map the renderer's invalid zero sentinel to None",
   );
   assert!(uploader.contains("fn release_rgba8(&mut self, handle: gfx_api::ImageHandle)"));
   assert!(uploader.contains("fn append_a8("));
   assert!(uploader.contains("(*self.renderer).image_append_a8(handle, x, y, w, h, data, row_bytes)"));
   assert!(uploader.contains("fn release_a8(&mut self, handle: gfx_api::ImageHandle)"));
   assert!(uploader.contains("(*self.renderer).image_release(handle)"));
   assert!(!uploader.contains("to_vec()") && !uploader.contains("swap("));
}

#[test]
fn app_installation_publishes_the_flag_after_the_owned_slot()
{
   let source = include_str!("../src/lib.rs");
   let install = source
      .split("pub fn install_app(")
      .nth(1)
      .expect("install_app")
      .split("pub unsafe fn run_app(")
      .next()
      .expect("install_app end");
   let slot = install.find("*slot = Some(app);").expect("owned app slot publication");
   let flag = install
      .find("INJECTED_APP_INSTALLED.store(true, Ordering::Release);")
      .expect("installed flag publication");

   assert!(slot < flag);
   assert!(install.contains("let state = lock_or_recover(app_state());"));
}
