#[test]
fn ios_product_renderer_selects_visible_frame_resource_depth()
{
   let source = include_str!("../src/lib.rs");
   assert!(source.contains("..metal::MetalRendererConfig::visible_host()"));
   assert!(source.contains("metal::MetalRenderer::new_with_config(renderer_cfg)"));
}
