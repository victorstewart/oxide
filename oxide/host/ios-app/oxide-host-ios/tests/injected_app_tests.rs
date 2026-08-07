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
   assert!(uploader.contains("(*self.renderer).image_release(handle)"));
   assert!(!uploader.contains("to_vec()") && !uploader.contains("swap("));
}
