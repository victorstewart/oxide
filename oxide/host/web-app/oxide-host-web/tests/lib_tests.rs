use oxide_host_web::generate_checker_rgba;

#[test]
fn checker_texture_has_expected_size_and_alpha() {
    let rgba = generate_checker_rgba(8, 4);
    assert_eq!(rgba.len(), 8 * 4 * 4);
    for pixel in rgba.chunks_exact(4) {
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn checker_texture_alternates_tiles() {
    let rgba = generate_checker_rgba(64, 24);
    let first = &rgba[0..4];
    let second_tile = &rgba[(24 * 4)..(25 * 4)];
    assert_ne!(first, second_tile);
}

#[test]
fn static_shell_imports_generated_pkg_and_platform_smoke_hook() {
    let html = include_str!("../../www/index.html");
    assert!(html.contains("./pkg/oxide_host_web.js"));
    assert!(html.contains("platform_smoke_report"));
    assert!(html.contains("webgpu_smoke_report"));
    assert!(html.contains("start_oxide_async"));
    assert!(html.contains("background: transparent"));
    assert!(html.contains("window.oxidePlatformSmoke"));
    assert!(html.contains("window.oxideWebGpuSmoke"));
    assert!(html.contains("window.oxideWebPerf"));
    assert!(html.contains("oxide-platform-smoke"));
    assert!(html.contains("oxide-webgpu-smoke"));
    assert!(html.contains("oxide-renderer-backend"));
    assert!(html.contains("oxide-render-smoke"));
    assert!(html.contains("oxide-web-perf"));
    assert!(html.contains("renderer_backend"));
    assert!(html.contains("last_draw_count"));
    assert!(html.contains("bench_frame_samples"));
}

#[test]
fn host_installs_browser_ime_bridge() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("compositionstart"));
    assert!(source.contains("compositionupdate"));
    assert!(source.contains("compositionend"));
    assert!(source.contains("oxide-ime-show"));
    assert!(source.contains("oxide-ime-hide"));
    assert!(source.contains("input_commit"));
}

#[test]
fn host_visual_startup_requires_async_webgpu() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("from_canvas_id_webgpu"));
    assert!(source.contains("webgpu renderer requires async browser initialization"));
    assert!(!source.contains("from_canvas_id_canvas2d"));
}
