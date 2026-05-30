use oxide_renderer_api as api;
use oxide_renderer_api::Renderer;
use oxide_renderer_web::{
    color_cache_key, color_to_css, layer_physical_dimension, packed_rgba_to_css, sanitize_scale,
    WebRenderer,
};

#[test]
fn color_conversion_clamps_channels() {
    let css = color_to_css(api::Color::rgba(1.4, -0.2, 0.5, 2.0));
    assert_eq!(css, "rgba(255, 0, 128, 1.000)");
    assert_eq!(packed_rgba_to_css(0x8040_2010), "rgba(16, 32, 64, 0.502)");
    assert_eq!(color_cache_key(api::Color::rgba(1.0, 0.0, 0.5, 0.25)), 0x4080_00FF);
}

#[test]
fn sanitize_scale_rejects_invalid_values() {
    assert_eq!(sanitize_scale(2.0), 2.0);
    assert_eq!(sanitize_scale(0.0), 1.0);
    assert_eq!(sanitize_scale(f32::NAN), 1.0);
}

#[test]
fn layer_physical_dimension_is_bounded_and_positive() {
    assert_eq!(layer_physical_dimension(12.25, 2.0), 25);
    assert_eq!(layer_physical_dimension(0.0, 2.0), 1);
    assert_eq!(layer_physical_dimension(f32::NAN, 2.0), 1);
    assert_eq!(layer_physical_dimension(100_000.0, 2.0), 16_384);
}

#[test]
fn native_stub_tracks_frame_shape_and_reports_unsupported_submit() {
    let mut renderer = WebRenderer::new_for_tests(100, 50, 2.0);
    let damage = api::Damage { rects: vec![api::RectI::new(0, 0, 10, 10)] };
    let token = renderer.begin_frame(&api::FrameTarget, Some(&damage));
    renderer.encode_pass(&api::DrawList::default());
    let stats = renderer.last_stats();
    assert_eq!(stats.frame_id, 1);
    assert_eq!(stats.width, 100);
    assert_eq!(stats.height, 50);
    assert_eq!(stats.scale, 2.0);
    assert_eq!(stats.damage_rects, 1);
    assert!(matches!(renderer.submit(token), Err(api::RenderError::Unsupported(_))));
}

#[test]
fn wasm_public_exports_are_webgpu_only() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("pub use wasm::{BrowserRenderer, WebGpuRenderer};"));
    assert!(!source.contains("pub use wasm::{BrowserRenderer, WebGpuRenderer, WebRenderer};"));
}

#[test]
fn wasm_webgpu_submits_directly_to_surface_without_backdrop_effects() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let renderer_impl =
        source.split("impl api::Renderer for WebGpuRenderer").nth(1).expect("webgpu renderer impl");
    let submit = renderer_impl.split("fn resize(&mut self").next().expect("webgpu submit body");

    assert!(submit.contains("if self.frame_uses_backdrop()"));
    assert!(submit.contains("self.render_scene_with_effects(&mut encoder);"));
    assert!(submit.contains("self.render_present(&mut encoder, &surface_view);"));
    assert!(submit.contains("self.render_direct(&mut encoder, &surface_view);"));
}

#[test]
fn wasm_webgpu_present_quad_uploads_are_cached_across_frames() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let cache = source
        .split("fn ensure_present_buffers")
        .nth(1)
        .expect("present buffer cache helper")
        .split("fn render_present")
        .next()
        .expect("present buffer cache body");
    let present = source
        .split("fn render_present")
        .nth(1)
        .expect("present pass body")
        .split("fn canvas_by_id")
        .next()
        .expect("present pass end");

    assert!(cache.contains("self.present_width == self.width"));
    assert!(cache.contains("self.queue.write_buffer(&self.present_vertex_buffer"));
    assert!(cache.contains("self.queue.write_buffer(&self.present_index_buffer"));
    assert!(present.contains("self.ensure_present_buffers();"));
    assert!(!present.contains("queue.write_buffer"));
}

#[test]
fn wasm_webgpu_unindexed_quad_vertices_emit_two_triangles() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let helper = source
        .split("fn append_gpu_vertices")
        .nth(1)
        .expect("append helper")
        .split("fn logical_dimension")
        .next()
        .expect("append helper end");

    assert!(helper.contains("vertices.len() == 4"));
    assert!(helper.contains("[base, base + 1, base + 2, base + 2, base + 1, base + 3]"));
}
