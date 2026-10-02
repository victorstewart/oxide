use oxide_host_web::generate_checker_rgba;
use std::io::Cursor;

#[test]
fn web_host_runtime_omits_accessibility_attributes()
{
   let source = include_str!("../src/lib.rs");
   let forbidden_prefix = ["ar", "ia-"].concat();
   assert!(!source.contains(&forbidden_prefix));
}

#[test]
fn browser_benchmark_host_explicitly_enables_renderer_diagnostics_and_snapshots()
{
   let manifest = include_str!("../Cargo.toml");
   assert!(manifest.contains(
      "features = [\"diagnostic-instrumentation\", \"snapshot-tests\"]",
   ));
}

fn decode_png_rgba(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("decode PNG header");
    let mut out = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out).expect("decode PNG pixels");
    let pixels = &out[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => {
            let mut converted = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for pixel in pixels.chunks_exact(3) {
                converted.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            converted
        }
        other => panic!("unsupported PNG color type {other:?}"),
    };
    (info.width, info.height, rgba)
}



fn source_fn_slice<'a>(source: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
    let start =
        source.find(start_marker).unwrap_or_else(|| panic!("missing source marker {start_marker}"));
    let tail = &source[start..];
    let end = tail.find(end_marker).unwrap_or(tail.len());
    &tail[..end]
}

#[test]
fn host_exposes_opt_in_webgpu_architecture_primitive_matrix() {
    let source = include_str!("../src/lib.rs");
    let page = include_str!("../../www/index.html");
    let method = source_fn_slice(
        source,
        "pub async fn bench_webgpu_architecture_primitives",
        "pub async fn bench_webgpu_direct_surface_ab",
    );

    for case in [
        "rrect_1",
        "rrect_64",
        "rrect_1024",
        "spinner_1",
        "spinner_64",
        "spinner_512",
        "neon_64",
        "neon_1024",
        "nine_slice_64",
        "nine_slice_512",
    ] {
        assert!(method.contains(case), "missing WebGPU architecture primitive {case}");
    }
    assert!(method.contains("architecture_primitive_frame"));
    assert!(method.contains("wait_renderer_queue_idle"));
    assert!(method.contains("settle_renderer_timestamps"));
    assert!(method.contains("sampled_case_metrics"));
    assert!(method.contains("current_gpu_samples"));
    assert!(method.contains("current_gpu_p99_ms"));
    assert!(source.contains("queue_completion_flag_for_benchmark"));
    assert!(source.contains("wait_animation_frame_once().await?"));
    assert!(page.contains("params.get(\"architecture_matrix\") === \"1\""));
    assert!(page.contains("bench_webgpu_architecture_primitives"));
    assert!(source.contains("pub async fn bench_webgpu_rrect_architecture"));
    assert!(method.contains("rrect_pathological_64"));
    assert!(method.contains("dpr={dpr:.1}"));
    assert!(page.contains("params.get(\"rrect_architecture_only\") === \"1\""));
    assert!(page.contains("bench_webgpu_rrect_architecture"));
    assert!(source.contains("pub fn render_webgpu_rrect_snapshot"));
    assert!(source.contains("fn rrect_capture_frame"));
    assert!(page.contains("captureTarget === \"rrect\""));
    assert!(include_str!("../../../../scripts/check_webgpu_browser_golden.mjs")
        .contains("--rrect-architecture-only"));
    assert!(source.contains("pub async fn bench_webgpu_image_architecture"));
    assert!(method.contains("image_mixed_1000"));
    assert!(page.contains("params.get(\"image_architecture_only\") === \"1\""));
    assert!(page.contains("bench_webgpu_image_architecture"));
    assert!(source.contains("pub fn render_webgpu_image_snapshot"));
    assert!(source.contains("fn image_capture_frame"));
    assert!(page.contains("captureTarget === \"image\""));
    assert!(include_str!("../../../../scripts/check_webgpu_browser_golden.mjs")
        .contains("--image-architecture-only"));
    assert!(source.contains("pub async fn bench_webgpu_nine_slice_architecture"));
    assert!(method.contains("nine_slice_1024"));
    assert!(page.contains("params.get(\"nine_slice_architecture_only\") === \"1\""));
    assert!(page.contains("bench_webgpu_nine_slice_architecture"));
    assert!(source.contains("pub fn render_webgpu_nine_slice_snapshot"));
    assert!(source.contains("fn nine_slice_capture_frame"));
    assert!(page.contains("captureTarget === \"nine-slice\""));
    assert!(include_str!("../../../../scripts/check_webgpu_browser_golden.mjs")
        .contains("--nine-slice-architecture-only"));
    assert!(source.contains("pub async fn bench_webgpu_spinner_architecture"));
    assert!(method.contains("spinner_1024"));
    assert!(page.contains("params.get(\"spinner_architecture_only\") === \"1\""));
    assert!(page.contains("bench_webgpu_spinner_architecture"));
    assert!(source.contains("pub fn render_webgpu_spinner_snapshot"));
    assert!(source.contains("fn spinner_capture_frame"));
    assert!(page.contains("captureTarget === \"spinner\""));
    assert!(page.contains("const runSpinnerRafHarness = async frameCount =>"));
    assert!(page.contains("browser-displayed-spinner-frames"));
    assert!(page.contains("runSpinnerRafHarness(Math.min(rafFrames, 600))"));
    assert!(page.contains("spinner_frame_perf: window.oxideWebGpuSpinnerFramePerf"));
    assert!(source.contains("renderer.set_animation_time_ms(timestamp_ms);"));
    assert!(include_str!("../../../../scripts/check_webgpu_browser_golden.mjs")
        .contains("--spinner-architecture-only"));
    assert!(source.contains("pub async fn bench_webgpu_neon_marker_architecture"));
    assert!(page.contains("params.get(\"neon_marker_architecture_only\") === \"1\""));
    assert!(page.contains("bench_webgpu_neon_marker_architecture"));
    assert!(source.contains("pub fn render_webgpu_neon_marker_snapshot"));
    assert!(source.contains("fn neon_marker_capture_frame"));
    assert!(page.contains("captureTarget === \"neon-marker\""));
    assert!(include_str!("../../../../scripts/check_webgpu_browser_golden.mjs")
        .contains("--neon-marker-architecture-only"));
}

#[test]
fn host_image_store_benchmark_uses_unique_sources_and_honest_timing_boundaries() {
    let source = include_str!("../src/lib.rs");
    let page = include_str!("../../www/index.html");
    let method = source_fn_slice(
        source,
        "pub async fn bench_webgpu_image_store",
        "pub async fn bench_webgpu_nine_slice_architecture",
    );

    assert!(source.contains("fn c60_web_icon_png"));
    assert!(method.contains("c60_web_icon_png(seed as u64, 64)"));
    assert!(method.contains("wait_renderer_queue_idle(&renderer).await?"));
    assert!(method.contains("wait_animation_frame_once().await?"));
    assert!(method.contains("settle_renderer_timestamps"));
    assert!(method.contains("request_to_first_displayed_frame_ms"));
    assert!(method.contains("store_request_to_first_publication_ms_avg"));
    assert!(method.contains("submit_p50_ms"));
    assert!(!method.contains("event_to_first_visible_ms"));
    assert!(!method.contains("frame_p50_ms"));
    assert!(page.contains("params.get(\"image_store_only\") === \"1\""));
    assert!(page.contains("bench_webgpu_image_store"));
}

#[test]
fn host_exposes_prepared_chunk_browser_contract()
{
   let source = include_str!("../src/lib.rs");
   let page = include_str!("../../www/index.html");
   assert!(source.contains("pub async fn bench_webgpu_prepared_chunks"));
   assert!(source.contains("pub async fn bench_webgpu_prepared_guardrails"));
   assert!(source.contains("WEBGPU_PREPARED_CHUNKS: usize = 256"));
   assert!(source.contains("WEBGPU_PREPARED_DRAW_COUNTS: [usize; 4] = [8, 16, 32, 64]"));
   assert!(source.contains("renderer.encode_snapshot(snapshot)"));
   assert!(source.contains("snapshot.flatten_into(flat)"));
   assert!(source.contains("cache_hits_avg"));
   assert!(source.contains("bundle_replays_avg"));
   assert!(source.contains("bundle_execute_calls_avg"));
   assert!(source.contains("active_frame_samples_ms"));
   assert!(source.contains("queue_wait_samples_ms"));
   assert!(source.contains("structural_bundle_creates"));
   assert!(source.contains("webgpu_prepared_structural_snapshot"));
   assert!(source.contains("budget_upload_bytes"));
   assert!(page.contains("params.get(\"prepared_only\") === \"1\""));
   assert!(page.contains("params.get(\"glyph_matrix_only\") === \"1\""));
   assert!(page.contains("params.get(\"glyph_run_only\") === \"1\""));
   assert!(page.contains("bench_webgpu_prepared_chunks"));
}

#[test]
fn host_exposes_local_layer_dimension_benchmark_and_edge_capture()
{
   let source = include_str!("../src/lib.rs");
   let page = include_str!("../../www/index.html");
   let runner = include_str!("../../../../scripts/run_webgpu_local_layers_c30.mjs");
   let capture = include_str!("../../../../scripts/check_webgpu_browser_golden.mjs");

   assert!(source.contains("WEBGPU_LOCAL_LAYER_CARDS: usize = 100"));
   assert!(source.contains("WEBGPU_LOCAL_LAYER_WIDTH: f32 = 72.0"));
   assert!(source.contains("WEBGPU_LOCAL_LAYER_HEIGHT: f32 = 40.0"));
   assert!(source.contains("WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_DRAWS: usize = 64"));
   assert!(source.contains("WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_FRAMES: usize = 12"));
   assert!(source.contains("WEBGPU_LOCAL_LAYER_GPU_POSTROLL_FRAMES: usize = 1"));
   assert!(source.contains("pub async fn bench_webgpu_local_layers_c30"));
   assert!(source.contains("pub async fn bench_webgpu_local_layer_guardrails_c30"));
   assert!(source.contains("pub async fn bench_webgpu_layer_cache_c31"));
   assert!(source.contains("pub fn render_webgpu_local_layers_c30"));
   assert!(source.contains("webgpu_local_layer_card_snapshots"));
   assert!(source.contains("webgpu_local_layer_edge_snapshots"));
   assert!(source.contains("webgpu_local_layer_resource_snapshot"));
   assert!(source.contains("expected_local_layer_bytes"));
   assert!(source.contains("full_canvas_layer_bytes"));
   assert!(source.contains("layer_clear_pixels_avg"));
   assert!(source.contains("gpu_samples_ms"));
   assert!(source.contains("warmup_samples_ms"));
   assert!(source.contains("gpu_clock_warmup_frames"));
   assert!(source.contains("gpu_postroll_frames"));
   assert!(source.contains("sample.frame_id != postroll_frame_id"));
   assert!(page.contains("captureTarget === \"local-layers\""));
   assert!(page.contains("render_webgpu_local_layers_c30"));
   assert!(runner.contains("bench_webgpu_local_layers_c30"));
   assert!(runner.contains("bench_webgpu_local_layer_guardrails_c30"));
   assert!(runner.contains("bench_webgpu_layer_cache_c31"));
   assert!(runner.contains("gpu_sample_count"));
   assert!(runner.contains("invalid C30 GPU sample population"));
   assert!(runner.contains("kern_num_files_before"));
   assert!(runner.contains("a prior C30 Chrome process is still running"));
   assert!(runner.contains("resource_update_misses"));
   assert!(capture.contains("assertLocalLayersRendered"));
}

#[test]
fn host_exposes_dynamic_property_browser_contract()
{
   let source = include_str!("../src/lib.rs");
   let runner = include_str!("../../../../scripts/run_webgpu_dynamic_c26.mjs");
   assert!(source.contains("WEBGPU_DYNAMIC_PROPERTY_NODES: usize = 300"));
   assert!(source.contains("pub async fn bench_webgpu_dynamic_properties"));
   assert!(source.contains("pub fn render_webgpu_dynamic_property_snapshot"));
   assert!(source.contains("webgpu_dynamic_property_instances"));
   assert!(source.contains("webgpu_dynamic_property_snapshot"));
   assert!(source.contains("property_upload_bytes_avg"));
   assert!(source.contains("property_records_updated_avg"));
   assert!(source.contains("geometry_upload_bytes_avg"));
   assert!(source.contains("event_to_submit_samples_ms"));
   assert!(runner.contains("requestAnimationFrame"));
   assert!(runner.contains("raf_frame_samples_ms"));
   assert!(runner.contains("render_webgpu_dynamic_property_snapshot"));
}




fn assert_webgpu_id_mask_pixels(width: u32, height: u32, rgba: &[u8]) {
    assert_eq!((width, height), (512, 512));

    let mut colorful_pixels = 0usize;
    let mut green_pixels = 0usize;
    let mut blue_pixels = 0usize;
    let mut bright_pixels = 0usize;
    let mut dark_pixels = 0usize;
    for pixel in rgba.chunks_exact(4) {
        let r = pixel[0];
        let g = pixel[1];
        let b = pixel[2];
        let a = pixel[3];
        assert_eq!(a, 255);
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        if hi.saturating_sub(lo) > 48 {
            colorful_pixels += 1;
        }
        if g > r.saturating_add(16) && g > b.saturating_add(16) {
            green_pixels += 1;
        }
        if b > r.saturating_add(20) && b > g.saturating_add(20) {
            blue_pixels += 1;
        }
        if r > 180 || g > 180 || b > 180 {
            bright_pixels += 1;
        }
        if r < 24 && g < 24 && b < 24 {
            dark_pixels += 1;
        }
    }

    assert!(colorful_pixels > 100000, "WebGPU ID-mask golden is not colorful enough");
    assert!(green_pixels > 25000, "WebGPU ID-mask golden is missing green city fills");
    assert!(blue_pixels > 50000, "WebGPU ID-mask golden is missing blue/purple city fills");
    assert!(bright_pixels > 5000, "WebGPU ID-mask golden is missing bright seam/edge pixels");
    assert!(
        bright_pixels < 80000,
        "WebGPU ID-mask golden looks like the app capture, not the compositor"
    );
    assert!(dark_pixels < 80000, "WebGPU ID-mask golden has too many untouched pixels");
}

fn assert_webgpu_scene3d_pixels(width: u32, height: u32, rgba: &[u8]) {
    let pixel_count = (width as usize).saturating_mul(height as usize);
    let mut colorful = 0usize;
    let mut blue = 0usize;
    let mut orange = 0usize;
    let mut dark = 0usize;
    for pixel in rgba.chunks_exact(4) {
        let r = pixel[0];
        let g = pixel[1];
        let b = pixel[2];
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        if hi > 48 && hi.saturating_sub(lo) > 36 {
            colorful += 1;
        }
        if b > r.saturating_add(36) && b > g.saturating_add(16) {
            blue += 1;
        }
        if r > b.saturating_add(36) && g > b.saturating_add(8) {
            orange += 1;
        }
        if r < 24 && g < 24 && b < 32 {
            dark += 1;
        }
    }

    assert!(colorful > pixel_count / 12, "WebGPU Scene3D golden is missing colored geometry");
    assert!(blue > pixel_count / 35, "WebGPU Scene3D golden is missing the blue back triangle");
    assert!(
        orange > pixel_count / 55,
        "WebGPU Scene3D golden is missing the orange front triangle"
    );
    assert!(dark > pixel_count / 3, "WebGPU Scene3D golden is missing the dark clear background");
}

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
    let source = include_str!("../src/lib.rs");
    assert!(html.contains("./pkg/oxide_host_web.js"));
    assert!(html.contains("OxideWebApp"));
    assert!(html.contains("platform_smoke_report"));
    assert!(html.contains("webgpu_smoke_report"));
    assert!(html.contains("webgpu_timing_report"));
    assert!(html.contains("bench_canvas_indexed_quads"));
    assert!(html.contains("start_oxide_async"));
    assert!(html.contains("background: transparent"));
    assert!(html.contains("window.oxidePlatformSmoke"));
    assert!(html.contains("window.oxideWebGpuSmoke"));
    assert!(html.contains("window.oxideWebGpuTiming"));
    assert!(html.contains("window.oxideWebPerf"));
    assert!(html.contains("window.oxideWebGpuIdMaskCurrent"));
    assert!(html.contains("window.oxideWebGpuUploadCurrent"));
    assert!(html.contains("window.oxideWebGpuScene3dAB"));
    assert!(html.contains("window.oxideWebGpuMixedMatrix"));
    assert!(html.contains("window.oxideWebGpuLayerEffectsMatrix"));
    assert!(html.contains("window.oxideWebGpuCommandFamilyMatrix"));
    assert!(html.contains("window.oxideWebGpuDrawItemCoalescingAB"));
    assert!(html.contains("window.oxideWebGpuDrawStateCacheAB"));
    assert!(html.contains("window.oxideWebGpuClipStateAB"));
    assert!(html.contains("window.oxideWebGpuEffectUniformAB"));
    assert!(html.contains("prewarm_webgpu_bench_resources"));
    assert!(html.contains("prewarm_webgpu_id_mask_bench_resources"));
    assert!(html.contains("oxide-webgpu-bench"));
    assert!(html.contains("oxide-canvas-bench"));
    assert!(html.contains("window.oxideCanvasIndexedQuads"));
    assert!(html.contains("oxide-canvas-indexed-quads"));
    assert!(html.contains("window.oxideWebBenchmarkMarks"));
    assert!(html.contains("benchmark_marks"));
    assert!(html.contains("performance.mark(start)"));
    assert!(html.contains("performance.measure(measure, start, end)"));
    assert!(html.contains("bench_timeout_ms"));
    assert!(html.contains("benchmark_error"));
    assert!(html.contains("typeof error.stack === \"string\""));
    assert!(html.contains("postErrorReport"));
    assert!(html.contains("wasmMemoryBytes"));
    assert!(html.contains("jsHeapSupported"));
    assert!(html.contains("collectJsHeapBytes"));
    assert!(html.contains("wasm_memory_before_bytes"));
    assert!(html.contains("wasm_memory_after_bytes"));
    assert!(html.contains("wasm_memory_growth_bytes"));
    assert!(html.contains("js_heap_sample_supported"));
    assert!(html.contains("js_heap_gc_available"));
    assert!(html.contains("js_heap_before_bytes"));
    assert!(html.contains("js_heap_after_bytes"));
    assert!(html.contains("js_heap_growth_bytes"));
    assert!(html.contains("window.oxideWebGpuAppSnapshot"));
    assert!(html.contains("window.oxideWebGpuScene3dSnapshot"));
    assert!(html.contains("window.oxideWebGpuIdMaskSnapshot"));
    assert!(html.contains("oxide-browser-report-json"));
    assert!(html.contains("await fetch(\"/__oxide_report\""));
    assert!(!html.contains("keepalive"));
    assert!(html.contains("startup_only"));
    assert!(html.contains("!captureOnly && !startupOnly"));
    assert!(html.contains("canvas_diag"));
    assert!(html.contains("canvas_samples"));
    assert!(html.contains("canvas_frames"));
    assert!(html.contains("canvas_quads"));
    assert!(html.contains("raf_frames"));
    assert!(html.contains("raf_resize_every"));
    assert!(html.contains("raf_scene"));
    assert!(html.contains("frame_at_timestamp_unprofiled"));
    assert!(html.contains("instrumentation_overhead"));
    assert!(html.contains("queue_drain_ms"));
    assert!(html.contains("event_update"));
    assert!(html.contains("draw_extraction"));
    assert!(html.contains("backend_lowering"));
    assert!(html.contains("command_encoding"));
    assert!(html.contains("submissions_per_raf: 1"));
    assert!(html.contains("backend: \"canvas2d\""));
    assert!(html.contains("frame_samples"));
    assert!(html.contains("id_mask_samples"));
    assert!(html.contains("upload_samples"));
    assert!(html.contains("scene3d_samples"));
    assert!(html.contains("mixed_samples"));
    assert!(html.contains("capture_target"));
    assert!(html.contains("capture_width"));
    assert!(html.contains("capture_height"));
    assert!(html.contains("benchmarkCanvas.style.width = `${captureWidth}px`"));
    assert!(html.contains("benchmarkCanvas.style.height = `${captureHeight}px`"));
    assert!(html.contains("canvas_css"));
    assert!(html.contains("canvas_physical"));
    assert!(html.contains("window.oxideApp.sync_canvas_metrics_for_benchmark();"));
    assert!(source.contains("pub fn sync_canvas_metrics_for_benchmark"));
    assert!(html.contains("capture_only"));
    assert!(html.contains("captureTarget === \"scene3d\""));
    assert!(html.contains("captureTarget === \"glyph\""));
    assert!(html.contains("captureTarget === \"id-mask\""));
    assert!(html.contains("await nextAnimationFrame();"));
    assert!(html.contains("oxide-platform-smoke"));
    assert!(html.contains("oxide-webgpu-smoke"));
    assert!(html.contains("oxide-webgpu-app-snapshot"));
    assert!(html.contains("oxide-webgpu-scene3d-snapshot"));
    assert!(html.contains("oxide-webgpu-id-mask-current"));
    assert!(html.contains("oxide-webgpu-upload-current"));
    assert!(html.contains("oxide-webgpu-effect-uniform-ab"));
    assert!(html.contains("oxide-webgpu-scene3d-ab"));
    assert!(html.contains("oxide-webgpu-mixed-matrix"));
    assert!(html.contains("oxide-webgpu-layer-effects-matrix"));
    assert!(html.contains("oxide-webgpu-command-family-matrix"));
    assert!(html.contains("oxide-webgpu-glyph-run-current"));
    assert!(html.contains("oxide-webgpu-glyph-language-matrix-current"));
    assert!(html.contains("oxide-webgpu-neon-marker-ab"));
    assert!(html.contains("oxide-webgpu-direct-surface-ab"));
    assert!(html.contains("oxide-webgpu-id-mask-snapshot"));
    assert!(html.contains("oxide-renderer-backend"));
    assert!(html.contains("oxide-render-smoke"));
    assert!(html.contains("oxide-web-cpu-submit-throughput"));
    assert!(html.contains("oxide-web-raf-frame-perf"));
    assert!(html.contains("runRafFrameHarness"));
    assert!(html.contains("raf_timestamps_ms"));
    assert!(html.contains("raf_deltas_ms"));
    assert!(html.contains("cpu_stages_ms"));
    assert!(html.contains("begin_raf_gpu_timestamp_capture"));
    assert!(html.contains("begin_raf_gpu_timestamp_capture(frameCount)"));
    assert!(html.contains("prewarm_raf_gpu_timestamp_capture_buffers(rafFrames)"));
    assert!(html.contains("prewarm_raf_gpu_timestamp_capture_buffers(idMaskCacheRafFrames)"));
    assert!(html.contains("prewarm_raf_gpu_timestamp_capture_buffers(Math.min(rafFrames, 600))"));
    assert!(html.contains("finish_raf_gpu_timestamp_capture"));
    assert!(html.contains("gpu_timestamp_samples"));
    assert!(html.contains("renderer_backend"));
    assert!(html.contains("last_draw_count"));
    assert!(html.contains("bench_cpu_submit_samples"));
    assert!(html.contains("frame_at_timestamp_profiled"));
    assert!(html.contains("bench_webgpu_id_mask_current"));
    assert!(html.contains("bench_webgpu_upload_current"));
    assert!(html.contains("bench_webgpu_effect_uniform_ab"));
    assert!(html.contains("bench_webgpu_backdrop_batch_current"));
    assert!(html.contains("bench_webgpu_backdrop_region_matrix"));
    assert!(html.contains("bench_webgpu_backdrop_region_case"));
    assert!(html.contains("bench_webgpu_backdrop_region_gpu_population"));
    assert!(html.contains("render_webgpu_backdrop_region_case"));
    assert!(html.contains("bench_webgpu_scene3d_ab"));
    assert!(html.contains("bench_webgpu_mixed_matrix"));
    assert!(html.contains("bench_webgpu_layer_effects_matrix"));
    assert!(html.contains("bench_webgpu_clean_layer_ab"));
    assert!(html.contains("bench_webgpu_command_family_matrix"));
    assert!(html.contains("bench_webgpu_glyph_run_current"));
    assert!(html.contains("bench_webgpu_glyph_language_matrix_current"));
    assert!(html.contains("bench_webgpu_neon_marker_ab"));
    assert!(html.contains("bench_webgpu_direct_surface_ab"));
    assert!(html.contains("render_webgpu_app_snapshot"));
    assert!(html.contains("render_webgpu_scene3d_snapshot"));
    assert!(html.contains("render_webgpu_glyph_snapshot"));
    assert!(html.contains("render_webgpu_id_mask_snapshot"));
    assert!(html.contains("app_snapshot"));
    assert!(html.contains("scene3d_snapshot"));
    assert!(html.contains("id_mask_snapshot"));
    assert!(html.contains("upload_current"));
    assert!(html.contains("effect_uniform_ab"));
    assert!(html.contains("backdrop_batch_current"));
    assert!(html.contains("backdrop_region_matrix"));
    assert!(html.contains("scene3d_ab"));
    assert!(html.contains("mixed_matrix"));
    assert!(html.contains("layer_effects_matrix"));
    assert!(html.contains("clean_layer_ab"));
    assert!(html.contains("command_family_matrix"));
    assert!(html.contains("glyph_run_current"));
    assert!(html.contains("glyph_language_matrix_current"));
    assert!(html.contains("neon_marker_ab"));
    assert!(html.contains("direct_surface_ab"));
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
    assert!(source.contains("BrowserRenderer::from_canvas_webgpu(canvas).await"));
    assert!(source.contains("webgpu renderer requires async browser initialization"));
    assert!(!source.contains("from_canvas_id_canvas2d"));
}

#[test]
fn host_sizes_the_canvas_before_webgpu_renderer_construction() {
    let source = include_str!("../src/lib.rs");
    let new_async = source
        .split("pub async fn new_async")
        .nth(1)
        .expect("async WebGPU constructor")
        .split("fn new_with_renderer")
        .next()
        .expect("async WebGPU constructor body");
    let backing_size = new_async.find("measure_canvas_metrics(&canvas)").expect("backing size");
    let set_width = new_async
        .find("canvas.set_width(metrics.physical_width)")
        .expect("canvas width");
    let set_height = new_async
        .find("canvas.set_height(metrics.physical_height)")
        .expect("canvas height");
    let renderer = new_async
        .find("BrowserRenderer::from_canvas_webgpu(canvas).await")
        .expect("WebGPU renderer construction");

    assert!(backing_size < set_width);
    assert!(set_width < set_height);
    assert!(set_height < renderer);
}

#[test]
fn host_exposes_webgpu_id_mask_ab_benchmark() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("pub fn bench_canvas_indexed_quads"));
    assert!(source.contains("oxide_renderer_web::bench_canvas_indexed_quads"));
    assert!(source.contains("create_hidden_canvas"));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_ab"));
    assert!(source.contains("pub fn prewarm_webgpu_id_mask_bench_resources"));
    assert!(source.contains("let result = webgpu_id_mask_frame("));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_current"));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_cache_c33"));
    assert!(source.contains("measure_webgpu_id_mask_multi_cache"));
    assert!(source.contains("one_entry_multi"));
    assert!(source.contains("lru_multi"));
    assert!(source.contains("pub async fn bench_webgpu_upload_current"));
    assert!(source.contains("pub async fn bench_webgpu_atlas_c15"));
    assert!(source.contains("pub async fn bench_webgpu_targets_c19"));
    assert!(!source.contains("pub async fn bench_webgpu_upload_ab"));
    assert!(source.contains("pub async fn bench_webgpu_upload_scratch_ab"));
    assert!(source.contains("pub async fn bench_webgpu_effect_uniform_ab"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_batch_current"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_region_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_region_case"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_region_gpu_population"));
    assert!(source.contains("pub fn render_webgpu_backdrop_region_case"));
    assert!(source.contains("pub async fn bench_webgpu_scene3d_ab"));
    assert!(source.contains("pub async fn bench_webgpu_mixed_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_layer_effects_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_clean_layer_ab"));
    assert!(source.contains("pub async fn bench_webgpu_command_family_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_glyph_run_current"));
    assert!(source.contains("pub async fn bench_webgpu_glyph_language_matrix_current"));
    assert!(source.contains("pub async fn bench_webgpu_neon_marker_ab"));
    assert!(source.contains("pub async fn bench_webgpu_direct_surface_ab"));
    assert!(source.contains("pub async fn bench_webgpu_draw_item_coalescing_ab"));
    assert!(source.contains("pub async fn bench_webgpu_draw_state_cache_ab"));
    assert!(!source.contains("pub async fn bench_webgpu_clip_state_ab"));
    assert!(source.contains("OXIDE_WASM_ALLOCATOR"));
    assert!(source.contains("oxide_wasm_alloc_counter::CountingAllocator"));
    assert!(source.contains("WebGpuAllocationSummary"));
    assert!(source.contains("WebGpuFrameStageAllocationSummary"));
    assert!(source.contains("frame_at_profiled"));
    assert!(source.contains("frame_at_timestamp_unprofiled"));
    assert!(source.contains("set_cpu_submit_timing_enabled_for_benchmark"));
    assert!(source.contains("settle_renderer_timestamps_diagnostic"));
    assert!(source.contains("queue_drain_ms"));
    assert!(source.contains("event_update_ms="));
    assert!(source.contains("backend_lowering_ms="));
    assert!(source.contains("command_encoding_ms="));
    assert!(source.contains("fn frame_stage_allocation_metrics"));
    assert!(source.contains("fn add_allocation_frame"));
    assert!(source.contains("fn allocation_metrics"));
    assert!(source.contains("oxide_wasm_alloc_counter::snapshot()"));
    assert!(source.contains("damage_rects: Vec<gfx::RectI>"));
    assert!(source.contains("take_damage_into(&mut self.damage_rects)"));
    assert!(source.contains("wasm_alloc_count={}"));
    assert!(source.contains("wasm_realloc_count={}"));
    assert!(source.contains("wasm_allocating_frames={}"));
    assert!(source.contains("WebGpuFrameStage::RouterDraw"));
    assert!(source.contains("WebGpuFrameStage::EncodePass"));
    assert!(source.contains("wasm_stage_{name}_alloc_count={}"));
    assert!(source.contains("pub fn render_webgpu_app_snapshot"));
    assert!(source.contains("pub fn render_webgpu_scene3d_snapshot"));
    assert!(source.contains("pub fn render_webgpu_glyph_snapshot"));
    assert!(source.contains("pub fn render_webgpu_id_mask_snapshot"));
    assert!(source.contains("pub async fn read_webgpu_asymmetric_id_mask_fields"));
    assert!(source.contains("webgpu_asymmetric_id_mask_frame"));
    assert!(source.contains("id_mask_snapshot_json"));
    assert!(source.contains("SNAPSHOT_TIMESTAMP_MS"));
    assert!(source.contains("pub fn render_webgpu_scene3d_snapshot("));
    assert!(source.contains("width: u32"));
    assert!(source.contains("height: u32"));
    assert!(source.contains("webgpu_scene3d_frame(&mut renderer, physical_w, physical_h, 1.0)"));
    assert!(source.contains("WebGpuScene3dBenchResources"));
    assert!(source.contains("WebGpuScene3dStressBenchResources"));
    assert!(source.contains("WebGpuScene3dStressRecreateResources"));
    assert!(source.contains("WEBGPU_SCENE3D_STRESS_INSTANCES"));
    assert!(source.contains("resources.frame(renderer)"));
    assert!(source.contains("stress_resources.frame(renderer)"));
    assert!(source.contains("stress_recreate.frame(renderer)"));
    assert!(source.contains("webgpu_scene3d_recreate_frame(renderer, 512, 512, 2.0)"));
    assert!(source.contains("recreate_over_reused"));
    assert!(source.contains("stress_recreate_over_reused"));
    assert!(source.contains("mesh3d_create_colored"));
    assert!(source.contains("encode_scene3d(&pass)"));
    assert!(source.contains("mesh3d_release(back)"));
    assert!(source.contains("mesh3d_release(front)"));
    assert!(source.contains("encode_neon_markers(&neon_marker::NeonMarkerPass"));
    assert!(source.contains("webgpu_fill_neon_markers"));
    assert!(source.contains("bench_webgpu_id_mask_case(&mut renderer, true"));
    assert!(source.contains("bench_webgpu_id_mask_case(&mut renderer, false"));
    assert!(source.contains("WebGpuUploadBenchResources"));
    assert!(source.contains("bench_webgpu_sampled_case"));
    assert!(source.contains("resources.glyph_frame(renderer, true)"));
    assert!(source.contains("resources.image_frame(renderer, true)"));
    assert!(source.contains("resources.upload_scratch_frame(renderer)"));
    assert!(source.contains("resources.effect_uniform_frame(renderer)"));
    assert!(source.contains("resources.backdrop_batch_frame(renderer)"));
    assert!(source.contains("resources.mixed_frame(renderer)"));
    assert!(source.contains("resources.layer_effects_frame(renderer)"));
    assert!(source.contains("resources.clean_layer_frame(renderer, false)"));
    assert!(!source.contains("resources.clean_layer_frame(renderer, true)"));
    assert!(source.contains("resources.command_family_frame(renderer)"));
    assert!(source.contains("resources.glyph_run_frame(renderer)"));
    assert!(source.contains("WebGpuGlyphMatrixResources"));
    assert!(source.contains("languages=latin,rtl,cjk,emoji"));
    assert!(source.contains("text::PagedAtlas::new(128, 128"));
    assert!(source.contains("resources.neon_marker_frame(renderer)"));
    assert!(source.contains("resources.direct_surface_frame(renderer)"));
    assert!(source.contains("resources.draw_state_cache_frame(renderer)"));
    assert!(!source.contains("resources.clip_state_frame(renderer)"));
    assert!(source.contains("fn upload_scratch_frame"));
    assert!(source.contains("fn effect_uniform_frame"));
    assert!(source.contains("fn backdrop_batch_frame"));
    assert!(source.contains("fn layer_effects_frame"));
    assert!(source.contains("fn clean_layer_frame"));
    assert!(source.contains("fn command_family_frame"));
    assert!(source.contains("fn glyph_run_frame"));
    assert!(source.contains("fn neon_marker_frame"));
    assert!(source.contains("fn direct_surface_frame"));
    assert!(source.contains("fn draw_state_cache_frame"));
    assert!(!source.contains("fn clip_state_frame"));
    assert!(source.contains("bench_resources: Option<WebGpuUploadBenchResources>"));
    assert!(source.contains("fn ensure_upload_bench_resources"));
    assert!(source.contains("fn with_upload_bench_resources"));
    assert!(source.contains("WEBGPU_LAYER_EFFECT_GLYPHS"));
    assert!(source.contains("WEBGPU_LAYER_EFFECT_IMAGE_TILES"));
    assert!(source.contains("WEBGPU_LAYER_EFFECT_IMAGE_COLUMNS"));
    assert!(source.contains("WEBGPU_LAYER_EFFECT_BACKDROPS"));
    assert!(source.contains("WEBGPU_EFFECT_UNIFORM_BACKDROPS"));
    assert!(source.contains("WEBGPU_BACKDROP_BATCH_BACKDROPS"));
    assert!(source.contains("WEBGPU_UPLOAD_SCRATCH_UPDATES"));
    assert!(source.contains("WEBGPU_COMMAND_FAMILY_SDF_GLYPHS"));
    assert!(source.contains("WEBGPU_COMMAND_FAMILY_SDF_RUNS"));
    assert!(source.contains("WEBGPU_COMMAND_FAMILY_REPEATS"));
    assert!(source.contains("WEBGPU_COMMAND_FAMILY_COLUMNS"));
    assert!(source.contains("WEBGPU_GLYPH_RUN_RUNS"));
    assert!(source.contains("WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN"));
    assert!(source.contains("WEBGPU_GLYPH_RUN_SDF_RUNS"));
    assert!(source.contains("WEBGPU_NEON_MARKERS"));
    assert!(source.contains("WEBGPU_NEON_MARKER_COLUMNS"));
    assert!(source.contains("WEBGPU_DIRECT_SURFACE_DRAWS"));
    assert!(source.contains("WEBGPU_DIRECT_SURFACE_COLUMNS"));
    assert!(source.contains("WEBGPU_DRAW_STATE_CACHE_DRAWS"));
    assert!(source.contains("WEBGPU_DRAW_STATE_CACHE_COLUMNS"));
    assert!(source.contains("WEBGPU_DRAW_ITEM_COALESCE_EXPECTED_ITEMS"));
    assert!(source.contains("expected_layers=3"));
    assert!(source.contains("expected_damage_rects=3"));
    assert!(source.contains("expected_backdrops={WEBGPU_LAYER_EFFECT_BACKDROPS}"));
    assert!(source.contains("expected_image_meshes={WEBGPU_COMMAND_FAMILY_REPEATS}"));
    assert!(source.contains("expected_nine_slices={WEBGPU_COMMAND_FAMILY_REPEATS}"));
    assert!(source.contains("expected_sdf_runs={WEBGPU_COMMAND_FAMILY_SDF_RUNS}"));
    assert!(source.contains("expected_camera_bg=0"));
    assert!(source.contains("expected_glyph_runs={WEBGPU_GLYPH_RUN_RUNS}"));
    assert!(source.contains("expected_glyphs_per_run={WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN}"));
    assert!(source.contains("expected_glyph_quads={expected_glyph_quads}"));
    assert!(source.contains("expected_sdf_runs={WEBGPU_GLYPH_RUN_SDF_RUNS}"));
    assert!(source.contains("expected_sdf_glyph_quads={}"));
    assert!(source.contains("expected_markers={WEBGPU_NEON_MARKERS}"));
    assert!(!source.contains("WEBGPU_NEON_MARKERS.saturating_mul(3)"));
    assert!(source.contains("expected_image_draws={WEBGPU_DIRECT_SURFACE_DRAWS}"));
    assert!(source.contains("WEBGPU_DIRECT_SURFACE_DRAWS.saturating_add(1)"));
    assert!(source.contains("expected_source_draw_items={WEBGPU_DRAW_STATE_CACHE_DRAWS}"));
    assert!(
        source.contains("expected_current_draw_items={WEBGPU_DRAW_ITEM_COALESCE_EXPECTED_ITEMS}")
    );
    assert!(source.contains("expected_draw_items={WEBGPU_DRAW_STATE_CACHE_DRAWS}"));
    assert!(source.contains("expected_backdrops={WEBGPU_EFFECT_UNIFORM_BACKDROPS}"));
    assert!(source.contains("expected_backdrops={WEBGPU_BACKDROP_BATCH_BACKDROPS}"));
    assert!(source.contains("set_draw_state_cache_enabled_for_benchmark"));
    assert!(source.contains("set_draw_item_coalescing_enabled_for_benchmark"));
    assert!(source.contains("set_image_upload_scratch_enabled_for_benchmark"));
    assert!(source.contains("set_effect_uniform_batch_enabled_for_benchmark"));
    assert!(source.contains("set_backdrop_batch_enabled_for_benchmark"));
    assert!(source.contains("set_direct_surface_enabled_for_benchmark"));
    assert!(source.contains("append_glyph_grid"));
    assert!(!source.contains("set_camera_background_rgba8"));
    assert!(!source.contains("builder.camera_bg("));
    assert!(source.contains("glyph_upload_a8"));
    assert!(source.contains("image_update_rgba8"));
    assert!(source.contains("image_upload_temp_allocs={}"));
    assert!(source.contains("image_upload_scratch_bytes={}"));
    assert!(source.contains("effect_uniform_writes={}"));
    assert!(source.contains("id_mask_uniform_writes={}"));
    assert!(source.contains("id_mask_uniform_bytes={}"));
    assert!(source.contains("id_mask_uniform_slots={}"));
    assert!(source.contains("current_warmup_ms={:.3}"));
    assert!(source.contains("webgpu_id_mask_frame(&mut renderer, &vertices, 1"));
    assert!(source.contains("renderer.encode_id_mask_gpu_compositor(&distractor)"));
    assert!(source.contains(r#"\"uniform_writes\":{}"#));
    assert!(source.contains(r#"\"cache_hits\":{}"#));
    assert!(source.contains(r#"\"raster_passes\":{}"#));
    assert!(source.contains("direct_capture_active"));
    assert!(source.contains("state.direct_capture_active = true"));
    assert!(source.contains("if state.direct_capture_active"));
    assert!(source.contains("current_p99_ms"));
    assert!(source.contains("legacy_avg_ms"));
    assert!(source.contains("webgpu_timing_report"));
    assert!(source.contains("webgpu_adapter_feature_supported(&adapter, \"timestamp-query\")"));
    assert!(source.contains("pub async fn bench_cpu_submit_samples"));
    assert!(source.contains("pub fn frame_at_timestamp_profiled"));
    assert!(source.contains("pub fn begin_raf_gpu_timestamp_capture"));
    assert!(source.contains("pub fn prewarm_raf_gpu_timestamp_capture_buffers"));
    assert!(source.contains("pub async fn finish_raf_gpu_timestamp_capture"));
    assert!(source.contains("timestamp_samples_json"));
    assert!(source.contains("timestamp_json: String"));
    assert!(source.contains("expected_samples.saturating_mul(320)"));
    assert!(source.contains("timestamp_samples_json_into"));
    assert!(source.contains("JsValue::from_str(&state.timestamp_json)"));
    assert!(!source.contains("{backend_stats}{pacing}{allocations}"));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_ab"));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_current"));
    assert!(source.contains("pub async fn bench_webgpu_upload_current"));
    assert!(!source.contains("pub async fn bench_webgpu_upload_ab"));
    assert!(source.contains("pub async fn bench_webgpu_effect_uniform_ab"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_batch_current"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_region_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_scene3d_ab"));
    assert!(source.contains("pub async fn bench_webgpu_mixed_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_command_family_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_glyph_run_current"));
    assert!(source.contains("pub async fn bench_webgpu_direct_surface_ab"));
    assert!(source.contains("settle_renderer_timestamps"));
    assert!(source.contains("WEBGPU_TIMESTAMP_SETTLE_RAFS"));
    assert!(source.contains("fn timestamp_stats_cover_row"));
    assert!(source.contains("let target_frame_id = renderer.borrow().last_stats().frame_id"));
    assert!(source.contains("stats.gpu_timestamp_frame_id > after_frame_id"));
    assert!(source.contains("stats.gpu_timestamp_passes == stats.render_passes"));
    assert!(source.contains("pending_timestamp_readbacks"));
    assert!(source.contains("pending_readbacks == 0"));
    assert!(source.contains("pending readbacks {}"));
    assert!(source.contains("WebGPU timestamp readback did not settle for row"));
    assert!(source.contains("collect_timestamp_readbacks"));
    assert!(source.contains("solid_tris={}"));
    assert!(source.contains("rrect_instances={}"));
    assert!(source.contains("rrect_triangles={}"));
    assert!(source.contains("rrect_instance_bytes={}"));
    assert!(source.contains("image_instances={}"));
    assert!(source.contains("image_triangles={}"));
    assert!(source.contains("image_instance_bytes={}"));
    assert!(source.contains("image_draws={}"));
    assert!(source.contains("image_mesh_draws={}"));
    assert!(source.contains("nine_slice_draws={}"));
    assert!(source.contains("nine_slice_instances={}"));
    assert!(source.contains("nine_slice_triangles={}"));
    assert!(source.contains("nine_slice_instance_bytes={}"));
    assert!(source.contains("spinner_instances={}"));
    assert!(source.contains("spinner_triangles={}"));
    assert!(source.contains("spinner_instance_bytes={}"));
    assert!(source.contains("neon_marker_instances={}"));
    assert!(source.contains("neon_marker_triangles={}"));
    assert!(source.contains("neon_marker_instance_bytes={}"));
    assert!(source.contains("glyph_quads={}"));
    assert!(source.contains("sdf_glyph_quads={}"));
    assert!(source.contains("clip_depth_peak={}"));
    assert!(source.contains("damage_rects={}"));
    assert!(source.contains("layer_draws={}"));
    assert!(source.contains("scene3d_draws={}"));
    assert!(source.contains("id_mask_draws={}"));
    assert!(source.contains("backdrop_draws={}"));
    assert!(source.contains("visual_effect_draws={}"));
    assert!(source.contains("spinner_draws={}"));
    assert!(source.contains("camera_bg_draws={}"));
    assert!(source.contains("render_passes={}"));
    assert!(source.contains("clear_passes={}"));
    assert!(source.contains("draw_passes={}"));
    assert!(source.contains("scene3d_passes={}"));
    assert!(source.contains("scene3d_overlay_passes={}"));
    assert!(source.contains("id_mask_raster_passes={}"));
    assert!(source.contains("id_mask_field_seed_passes={}"));
    assert!(source.contains("id_mask_field_jump_passes={}"));
    assert!(source.contains("id_mask_compositor_passes={}"));
    assert!(source.contains("present_passes={}"));
    assert!(source.contains("texture_copies={}"));
    assert!(source.contains("command_buffers={}"));
    assert!(source.contains("gpu_timestamp_query_supported={}"));
    assert!(source.contains("gpu_timestamp_total_ns={}"));
    assert!(source.contains("gpu_timestamp_backdrop_copy_ns={}"));
    assert!(source.contains("gpu_timestamp_id_mask_field_jump_ns={}"));
    assert!(source.contains("gpu_timestamp_readback_skips={}"));
    assert!(source.contains("gpu_timestamp_readback_interval={}"));
    assert!(source.contains("buffer_upload_bytes={}"));
    assert!(source.contains("texture_upload_bytes={}"));
    assert!(source.contains("buffer_grows={}"));
    assert!(source.contains("texture_creates={}"));
    assert!(source.contains("bind_group_creates={}"));
    assert!(source.contains("pipeline_creates={}"));
    assert!(source.contains("sampler_creates={}"));
    assert!(source.contains("mesh3d_creates={}"));
    assert!(source.contains("draw_buffer_grows={}"));
    assert!(source.contains("image_texture_creates={}"));
    assert!(source.contains("image_bind_group_creates={}"));
    assert!(source.contains("target_texture_creates={}"));
    assert!(source.contains("target_bind_group_creates={}"));
    assert!(source.contains("layer_texture_creates={}"));
    assert!(source.contains("layer_bind_group_creates={}"));
    assert!(source.contains("scene3d_buffer_grows={}"));
    assert!(source.contains("scene3d_bind_group_creates={}"));
    assert!(source.contains("effect_buffer_grows={}"));
    assert!(source.contains("effect_bind_group_creates={}"));
    assert!(source.contains("id_mask_texture_creates={}"));
    assert!(source.contains("id_mask_buffer_grows={}"));
    assert!(source.contains("id_mask_bind_group_creates={}"));
    assert!(source.contains("image_upload_temp_allocs={}"));
    assert!(source.contains("image_upload_temp_bytes={}"));
    assert!(source.contains("image_upload_scratch_bytes={}"));
    assert!(source.contains("image_upload_scratch_grows={}"));
    assert!(source.contains("cpu_scratch_bytes={}"));
    assert!(source.contains("cpu_scratch_grows={}"));
    assert!(source.contains("cpu_scratch_growth_bytes={}"));
    assert!(source.contains("cpu_draw_scratch_bytes={}"));
    assert!(source.contains("cpu_draw_scratch_grows={}"));
    assert!(source.contains("cpu_draw_scratch_growth_bytes={}"));
    assert!(source.contains("cpu_scene3d_scratch_bytes={}"));
    assert!(source.contains("cpu_scene3d_scratch_grows={}"));
    assert!(source.contains("cpu_scene3d_scratch_growth_bytes={}"));
    assert!(source.contains("cpu_effect_scratch_bytes={}"));
    assert!(source.contains("cpu_effect_scratch_grows={}"));
    assert!(source.contains("cpu_effect_scratch_growth_bytes={}"));
    assert!(source.contains("cpu_id_mask_scratch_bytes={}"));
    assert!(source.contains("cpu_id_mask_scratch_grows={}"));
    assert!(source.contains("cpu_id_mask_scratch_growth_bytes={}"));
    assert!(source.contains("cpu_image_upload_scratch_bytes={}"));
    assert!(source.contains("cpu_image_upload_scratch_grows={}"));
    assert!(source.contains("cpu_image_upload_scratch_growth_bytes={}"));
    assert!(source.contains("cpu_resource_table_scratch_bytes={}"));
    assert!(source.contains("cpu_resource_table_scratch_grows={}"));
    assert!(source.contains("cpu_resource_table_scratch_growth_bytes={}"));
    assert!(source.contains("commands_traversed={}"));
    assert!(source.contains("geometry_bytes_copied={}"));
    assert!(source.contains("actual_submissions={}"));
    assert!(source.contains("gpu_logical_total_bytes={}"));
    assert!(source.contains("gpu_allocated_total_bytes={}"));
    assert!(source.contains("gpu_scene3d_mesh_bytes={}"));
    assert!(source.contains("submit_allocation_metrics(&summary.submit_allocations"));
    assert!(source.contains("fn add_submit_allocation_frame"));
    assert!(source.contains("submit_surface_alloc_count"));
    assert!(source.contains("submit_finish_queue_alloc_count"));
    assert!(source.contains("submit_timestamp_map_alloc_count"));
    assert!(source.contains("fn renderer_stats_metrics"));
    assert!(source.contains("renderer_stats_metrics(current.stats, \"current\")"));
    assert!(source.contains("renderer_stats_metrics(legacy.stats, \"legacy\")"));
    assert!(source.contains("mixed_damage: gfx::Damage"));
    assert!(source.contains("layer_effects_damage: gfx::Damage"));
    assert!(source.contains("Some(&self.mixed_damage)"));
    assert!(source.contains("Some(&self.layer_effects_damage)"));
    let mixed_frame = source_fn_slice(source, "fn mixed_frame", "fn layer_effects_frame");
    let layer_effects_frame =
        source_fn_slice(source, "fn layer_effects_frame", "fn clean_layer_frame");
    let clean_layer_frame =
        source_fn_slice(source, "fn clean_layer_frame", "fn command_family_frame");
    let neon_marker_frame =
        source_fn_slice(source, "fn neon_marker_frame", "fn draw_state_cache_frame");
    assert!(!mixed_frame.contains("vec!["));
    assert!(!layer_effects_frame.contains("vec!["));
    assert!(!clean_layer_frame.contains("vec!["));
    assert!(!neon_marker_frame.contains("vec!["));
    assert!(source.contains("{key_prefix}render_passes={}"));
    assert!(source.contains("{key_prefix}clear_passes={}"));
    assert!(source.contains("{key_prefix}id_mask_field_jump_passes={}"));
    assert!(source.contains("{key_prefix}texture_copies={}"));
    assert!(source.contains("{key_prefix}gpu_timestamp_passes={}"));
    assert!(source.contains("{key_prefix}gpu_timestamp_readback_interval={}"));
    assert!(source.contains("{key_prefix}image_mesh_draws={}"));
    assert!(source.contains("{key_prefix}nine_slice_draws={}"));
    assert!(source.contains("{key_prefix}nine_slice_instances={}"));
    assert!(source.contains("{key_prefix}nine_slice_triangles={}"));
    assert!(source.contains("{key_prefix}nine_slice_instance_bytes={}"));
    assert!(source.contains("{key_prefix}spinner_instances={}"));
    assert!(source.contains("{key_prefix}spinner_triangles={}"));
    assert!(source.contains("{key_prefix}spinner_instance_bytes={}"));
    assert!(source.contains("{key_prefix}neon_marker_instances={}"));
    assert!(source.contains("{key_prefix}neon_marker_triangles={}"));
    assert!(source.contains("{key_prefix}neon_marker_instance_bytes={}"));
    assert!(source.contains("{key_prefix}sdf_glyph_quads={}"));
    assert!(source.contains("{key_prefix}layer_draws={}"));
    assert!(source.contains("{key_prefix}layer_cache_hits={}"));
    assert!(source.contains("{key_prefix}layer_cache_misses={}"));
    assert!(source.contains("{key_prefix}layer_cache_skipped_draws={}"));
    assert!(source.contains("{key_prefix}layer_passes={}"));
    assert!(source.contains("{key_prefix}scene3d_draws={}"));
    assert!(source.contains("{key_prefix}backdrop_draws={}"));
    assert!(source.contains("{key_prefix}buffer_upload_bytes={}"));
    assert!(source.contains("{key_prefix}image_upload_temp_allocs={}"));
    assert!(source.contains("{key_prefix}image_upload_scratch_bytes={}"));
    assert!(!source.contains("frame_pacing_metrics"));
    assert!(!source.contains("missed_frame_ratio_{refresh_hz}hz"));
    assert!(!source.contains("hitch_ratio_{refresh_hz}hz"));
    assert!(source.contains("vertex_revision: revision"));
    assert!(source.contains("sampled_case_metrics(&glyph_current, \"glyph_current\")"));
    assert!(source.contains("sampled_case_metrics(&image_current, \"image_current\")"));
}

#[test]
fn committed_webgpu_browser_golden_is_present_and_sized() {
    let png = include_bytes!("../../../../goldens/snapshots/webgpu_browser.png");
    assert!(png.len() > 1024, "webgpu browser golden should contain rendered canvas pixels");
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    assert_eq!((width, height), (320, 240));
}

#[test]
fn committed_webgpu_id_mask_golden_is_present_and_sized() {
    let png = include_bytes!("../../../../goldens/snapshots/webgpu_id_mask_compositor.png");
    assert!(png.len() > 1024, "webgpu ID-mask golden should contain rendered canvas pixels");
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    assert_eq!((width, height), (512, 512));
}

#[test]
fn committed_webgpu_glyph_golden_is_present_and_sized() {
    let png = include_bytes!("../../../../goldens/snapshots/webgpu_glyph_atlas.png");
    assert!(png.len() > 1024, "webgpu glyph golden should contain rendered atlas pixels");
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    assert_eq!((width, height), (512, 512));
}

#[test]
fn committed_webgpu_scene3d_golden_is_present_and_sized() {
    let cases: [(&[u8], (u32, u32), &str); 3] = [
        (
            include_bytes!("../../../../goldens/snapshots/webgpu_scene3d.png"),
            (512, 512),
            "webgpu_scene3d.png",
        ),
        (
            include_bytes!("../../../../goldens/snapshots/webgpu_scene3d_wide.png"),
            (640, 360),
            "webgpu_scene3d_wide.png",
        ),
        (
            include_bytes!("../../../../goldens/snapshots/webgpu_scene3d_portrait.png"),
            (360, 640),
            "webgpu_scene3d_portrait.png",
        ),
    ];
    for (png, size, name) in cases {
        assert!(png.len() > 1024, "{name} should contain rendered canvas pixels");
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert_eq!((width, height), size, "{name} has wrong PNG dimensions");
    }
}

#[test]
fn committed_webgpu_browser_golden_contains_rendered_pixels() {
    let png = include_bytes!("../../../../goldens/snapshots/webgpu_browser.png");
    let (width, height, rgba) = decode_png_rgba(png);
    assert_eq!((width, height), (320, 240));

    let mut blue_pixels = 0usize;
    let mut dark_pixels = 0usize;
    let mut background_pixels = 0usize;
    let mut opaque_pixels = 0usize;
    let mut transparent_pixels = 0usize;
    for pixel in rgba.chunks_exact(4) {
        let r = pixel[0];
        let g = pixel[1];
        let b = pixel[2];
        let a = pixel[3];
        if a == 255 {
            opaque_pixels += 1;
        } else if a == 0 {
            transparent_pixels += 1;
        }
        if a == 255 && b > 180 && r < 120 && g > 80 {
            blue_pixels += 1;
        }
        if a > 0 && r < 64 && g < 64 && b < 64 {
            dark_pixels += 1;
        }
        if a == 255 && r > 235 && g > 235 && b > 235 {
            background_pixels += 1;
        }
    }

    assert!(blue_pixels > 4000, "WebGPU golden is missing the blue control surfaces");
    assert!(dark_pixels > 20, "WebGPU golden is missing dark text and control details");
    assert!(background_pixels > 20000, "WebGPU golden is missing the light scene background");
    assert!(opaque_pixels > 40000, "WebGPU golden is missing the opaque rendered scene");
    assert!(transparent_pixels > 3000, "WebGPU golden is missing the transparent page exterior");
}

#[test]
fn committed_webgpu_id_mask_golden_contains_rendered_pixels() {
    let png = include_bytes!("../../../../goldens/snapshots/webgpu_id_mask_compositor.png");
    let (width, height, rgba) = decode_png_rgba(png);
    assert_webgpu_id_mask_pixels(width, height, &rgba);
}

#[test]
fn committed_webgpu_glyph_golden_contains_a8_and_sdf_pixels() {
    let png = include_bytes!("../../../../goldens/snapshots/webgpu_glyph_atlas.png");
    let (width, height, rgba) = decode_png_rgba(png);
    assert_eq!((width, height), (512, 512));
    let mut bright = 0usize;
    let mut cyan = 0usize;
    let mut dark = 0usize;
    for pixel in rgba.chunks_exact(4) {
        let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        assert_eq!(a, 255);
        bright += usize::from(r > 180 && g > 180 && b > 180);
        cyan += usize::from(b > 180 && g > 150 && r < 180);
        dark += usize::from(r < 24 && g < 28 && b < 36);
    }
    assert!(bright > 5000, "A8 rows are missing from the WebGPU glyph golden");
    assert!(cyan > 1000, "SDF rows are missing from the WebGPU glyph golden");
    assert!(dark > 100000, "glyph golden is missing its dark background");
}

#[test]
fn committed_webgpu_scene3d_golden_contains_rendered_pixels() {
    for png in [
        include_bytes!("../../../../goldens/snapshots/webgpu_scene3d.png").as_slice(),
        include_bytes!("../../../../goldens/snapshots/webgpu_scene3d_wide.png").as_slice(),
        include_bytes!("../../../../goldens/snapshots/webgpu_scene3d_portrait.png").as_slice(),
    ] {
        let (width, height, rgba) = decode_png_rgba(png);
        assert_webgpu_scene3d_pixels(width, height, &rgba);
    }
}

#[test]
fn webgpu_browser_capture_script_compares_pixels_against_golden() {
    let script = include_str!("../../../../scripts/check_webgpu_browser_golden.mjs");
    assert!(script.contains("--enable-unsafe-webgpu"));
    assert!(script.contains("--enable-precise-memory-info"));
    assert!(script.contains("--js-flags=--expose-gc"));
    assert!(script.contains("--screenshot"));
    assert!(script.contains("goldens\", \"snapshots\", \"webgpu_browser.png"));
    assert!(script.contains("webgpu_id_mask_compositor.png"));
    assert!(script.contains("webgpu_scene3d.png"));
    assert!(script.contains("webgpu_glyph_atlas.png"));
    assert!(script.contains("--target"));
    assert!(script.contains("function comparePngs"));
    assert!(script.contains("function assertIdMaskRendered"));
    assert!(script.contains("function assertScene3dRendered"));
    assert!(script.contains("pixelTolerance"));
    assert!(script.contains("golden mismatch"));
    assert!(script.contains("capture_target"));
    assert!(script.contains("capture_width"));
    assert!(script.contains("capture_height"));
    assert!(script.contains("capture_only"));
    assert!(script.contains("--capture-retries"));
    assert!(script.contains("captureAndCompare"));
    assert!(script.contains("retrying WebGPU browser capture attempt"));
    assert!(script.contains("rmSync(out, { force: true })"));
    assert!(script.contains("stableChecks < 2"));
    assert!(script.contains("child.kill(\"SIGTERM\")"));
    assert!(script.contains("Chrome did not write a screenshot within"));
    assert!(script.contains("webgpu_timing"));
    assert!(script.contains("gpu_stage_attribution"));
    assert!(script.contains("--trace-json"));
    assert!(script.contains("--trace-startup="));
    assert!(script.contains("browser_trace"));
    assert!(script.contains("capture_phase = \"benchmark-report\""));
    assert!(script.contains("timing_source = \"untraced-baseline-report\""));
    assert!(script.contains("benchmark_trace_interval_count"));
    assert!(script.contains("benchmark_trace_interval_labels"));
    assert!(script.contains("benchmark_trace_intervals"));
    assert!(script.contains("traceBenchmarkIntervals"));
    assert!(script.contains("Browser Trace"));
    assert!(script.contains("browser_startup"));
    assert!(script.contains("function webPackageStats"));
    assert!(script.contains("--startup-report"));
    assert!(script.contains("--startup-repeats"));
    assert!(script.contains("--image-store-only"));
    assert!(script.contains("--image-store-count"));
    assert!(script.contains("--image-store-standalone"));
    assert!(script.contains("image_store_only"));
    assert!(script.contains("image_store_count"));
    assert!(script.contains("image_store_standalone"));
    assert!(script.contains("startup_only"));
    assert!(script.contains("--canvas-report"));
    assert!(script.contains("--canvas-repeats"));
    assert!(script.contains("--canvas-samples"));
    assert!(script.contains("--canvas-frames"));
    assert!(script.contains("--canvas-quads"));
    assert!(script.contains("canvas_diag=1"));
    assert!(script.contains("function canvasDiagnosticReport"));
    assert!(script.contains("web.wasm.canvas.indexed_quads"));
    assert!(script.contains("web.wasm.canvas.browser_startup"));
    assert!(script.contains("function startupRepeatReport"));
    assert!(script.contains("web.wasm.webgpu.browser_startup_repeats"));
    assert!(script.contains("Browser Startup"));
    assert!(script.contains("package_bytes"));
    assert!(script.contains("upload_samples"));
    assert!(script.contains("scene3d_samples"));
    assert!(script.contains("mixed_samples"));
    assert!(script.contains("app_snapshot"));
    assert!(script.contains("scene3d_snapshot"));
    assert!(script.contains("id_mask_snapshot"));
    assert!(script.contains("--id-mask-reference-out"));
    assert!(script.contains("id_mask_reference_only"));
    assert!(script.contains("--glyph-matrix-out"));
    assert!(script.contains("glyph_matrix_only"));
    assert!(script.contains("--glyph-run-out"));
    assert!(script.contains("glyph_run_only"));
    assert!(script.contains("upload_current"));
    assert!(script.contains("backdrop_batch_current"));
    assert!(script.contains("--backdrop-region-out"));
    assert!(script.contains("--backdrop-region-gpu-out"));
    assert!(script.contains("backdrop_region_only"));
    assert!(script.contains("scene3d_ab"));
    assert!(script.contains("mixed_matrix"));
    assert!(script.contains("layer_effects_matrix"));
    assert!(script.contains("clean_layer_ab"));
    assert!(script.contains("command_family_matrix"));
    assert!(script.contains("glyph_run_current"));
    assert!(script.contains("neon_marker_ab"));
    assert!(script.contains("--json-report"));
    assert!(script.contains("--markdown-report"));
    assert!(script.contains("web.wasm.webgpu.id_mask_compositor.current"));
    assert!(script.contains("web.wasm.webgpu.glyph_atlas_upload.current_dirty"));
    assert!(script.contains("web.wasm.webgpu.image_upload.current_dirty"));
    assert!(script.contains("web.wasm.webgpu.effect_uniform.current_batched"));
    assert!(!script.contains("rows: [\"web.wasm.webgpu.effect_uniform.current_batched\", \"web.wasm.webgpu.effect_uniform.legacy_write_each\"]"));
    assert!(script.contains("web.wasm.webgpu.backdrop_batch.current_coalesced"));
    assert!(!script.contains("web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy"));
    assert!(script.contains("web.wasm.webgpu.scene3d.reused_mesh"));
    assert!(script.contains("web.wasm.webgpu.scene3d.recreate_mesh"));
    assert!(script.contains("web.wasm.webgpu.scene3d.stress_reused_mesh"));
    assert!(script.contains("web.wasm.webgpu.scene3d.stress_recreate_mesh"));
    assert!(script.contains("web.wasm.webgpu.mixed_text_image_effects"));
    assert!(!script.contains("rows: [\"web.wasm.webgpu.mixed_text_image_effects\", \"web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched\"]"));
    assert!(script.contains("web.wasm.webgpu.layer_damage_effects"));
    assert!(!script.contains("web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched"));
    assert!(script.contains("web.wasm.webgpu.clean_layer.clean_reuse"));
    assert!(script.contains("clean-layer dirty rerender row must stay retired"));
    assert!(!script.contains("rows: [\"web.wasm.webgpu.clean_layer.clean_reuse\", \"web.wasm.webgpu.clean_layer.dirty_rerender\"]"));
    assert!(script.contains("web.wasm.webgpu.command_family_matrix"));
    assert!(!script.contains("web.wasm.webgpu.command_family_matrix.legacy_rebind"));
    assert!(script.contains("web.wasm.webgpu.glyph_run.current"));
    assert!(!script.contains("web.wasm.webgpu.glyph_run.legacy_rebind"));
    assert!(script.contains("web.wasm.webgpu.neon_marker.current"));
    assert!(!script.contains("rows: [\"web.wasm.webgpu.neon_marker.current\", \"web.wasm.webgpu.neon_marker.legacy_rebind\"]"));
    assert!(script.contains("neon-marker legacy row must stay retired"));
    assert!(script.contains("web.wasm.webgpu.direct_surface.current"));
    assert!(!script.contains("rows: [\"web.wasm.webgpu.direct_surface.current\", \"web.wasm.webgpu.direct_surface.legacy_scene_present\"]"));
    assert!(script.contains("direct-surface legacy row must stay retired"));
    assert!(script.contains("effect_uniform_summary"));
    assert!(script.contains("backdrop_batch_summary"));
    assert!(script.contains("mixed_summary"));
    assert!(script.contains("layer_effects_summary"));
    assert!(script.contains("clean_layer_summary"));
    assert!(script.contains("command_family_summary"));
    assert!(script.contains("glyph_run_summary"));
    assert!(script.contains("neon_marker_summary"));
    assert!(script.contains("direct_surface_summary"));
    assert!(script.contains("Mixed Scene Summary"));
    assert!(script.contains("Layer Effects Summary"));
    assert!(script.contains("Clean Layer Summary"));
    assert!(script.contains("Command Family Summary"));
    assert!(script.contains("Glyph Run Summary"));
    assert!(script.contains("Neon Marker Summary"));
    assert!(script.contains("Direct Surface Summary"));
    assert!(script.contains("recreate_over_reused"));
    assert!(script.contains("warmResourceChurnSummary"));
    assert!(script.contains("warm_resource_churn"));
    assert!(script.contains("WARM_RESOURCE_CHURN_FIELDS"));
    assert!(script.contains("Warm Resource Churn"));
    assert!(script.contains("row_detail_count"));
    assert!(script.contains("row_details"));
    assert!(script.contains("Warm Resource Churn Rows"));
    assert!(script.contains("WEBGPU_BACKEND_PATHS"));
    assert!(script.contains("backend_path_coverage"));
    assert!(script.contains("Backend Path Coverage"));
    assert!(script.contains("solid_tris: numberMetric(metrics, \"solid_tris\")"));
    assert!(script.contains("rrect_instances: numberMetric(metrics, \"rrect_instances\")"));
    assert!(script.contains("rrect_triangles: numberMetric(metrics, \"rrect_triangles\")"));
    assert!(script.contains("rrect_instance_bytes: numberMetric(metrics, \"rrect_instance_bytes\")"));
    assert!(script.contains("image_instances: numberMetric(metrics, \"image_instances\")"));
    assert!(script.contains("image_triangles: numberMetric(metrics, \"image_triangles\")"));
    assert!(script.contains("image_instance_bytes: numberMetric(metrics, \"image_instance_bytes\")"));
    assert!(script.contains("draw_items: numberMetric(metrics, \"draw_items\")"));
    assert!(
        script.contains("draw_items_coalesced: numberMetric(metrics, \"draw_items_coalesced\")")
    );
    assert!(script.contains("draw_pipeline_binds: numberMetric(metrics, \"draw_pipeline_binds\")"));
    assert!(
        script.contains("draw_bind_group_binds: numberMetric(metrics, \"draw_bind_group_binds\")")
    );
    assert!(script.contains("draw_scissor_sets: numberMetric(metrics, \"draw_scissor_sets\")"));
    assert!(script.contains("image_draws: numberMetric(metrics, \"image_draws\")"));
    assert!(script.contains("image_tiles"));
    assert!(script.contains("image_mesh_draws: numberMetric(metrics, \"image_mesh_draws\")"));
    assert!(script.contains("nine_slice_draws: numberMetric(metrics, \"nine_slice_draws\")"));
    assert!(script.contains("nine_slice_instances: numberMetric(metrics, \"nine_slice_instances\")"));
    assert!(script.contains("nine_slice_triangles: numberMetric(metrics, \"nine_slice_triangles\")"));
    assert!(script.contains("nine_slice_instance_bytes: numberMetric(metrics, \"nine_slice_instance_bytes\")"));
    assert!(script.contains("spinner_instances: numberMetric(metrics, \"spinner_instances\")"));
    assert!(script.contains("spinner_triangles: numberMetric(metrics, \"spinner_triangles\")"));
    assert!(script.contains("spinner_instance_bytes: numberMetric(metrics, \"spinner_instance_bytes\")"));
    assert!(script.contains("neon_marker_instances: numberMetric(metrics, \"neon_marker_instances\")"));
    assert!(script.contains("neon_marker_triangles: numberMetric(metrics, \"neon_marker_triangles\")"));
    assert!(script.contains("neon_marker_instance_bytes: numberMetric(metrics, \"neon_marker_instance_bytes\")"));
    assert!(script.contains("glyph_quads: numberMetric(metrics, \"glyph_quads\")"));
    assert!(script.contains("sdf_glyph_quads: numberMetric(metrics, \"sdf_glyph_quads\")"));
    assert!(script.contains("clip_depth_peak: numberMetric(metrics, \"clip_depth_peak\")"));
    assert!(script.contains("damage_rects: numberMetric(metrics, \"damage_rects\")"));
    assert!(script.contains("layer_draws: numberMetric(metrics, \"layer_draws\")"));
    assert!(script.contains("layer_cache_hits: numberMetric(metrics, \"layer_cache_hits\")"));
    assert!(script.contains("layer_cache_misses: numberMetric(metrics, \"layer_cache_misses\")"));
    assert!(script.contains(
        "layer_cache_skipped_draws: numberMetric(metrics, \"layer_cache_skipped_draws\")"
    ));
    assert!(script.contains("layer_passes: numberMetric(metrics, \"layer_passes\")"));
    assert!(script.contains("scene3d_draws: numberMetric(metrics, \"scene3d_draws\")"));
    assert!(script.contains("id_mask_draws: numberMetric(metrics, \"id_mask_draws\")"));
    assert!(script.contains("backdrop_draws: numberMetric(metrics, \"backdrop_draws\")"));
    assert!(script.contains("visual_effect_draws: numberMetric(metrics, \"visual_effect_draws\")"));
    assert!(
        script.contains("effect_uniform_writes: numberMetric(metrics, \"effect_uniform_writes\")")
    );
    assert!(
        script.contains("effect_uniform_bytes: numberMetric(metrics, \"effect_uniform_bytes\")")
    );
    assert!(
        script.contains("effect_uniform_slots: numberMetric(metrics, \"effect_uniform_slots\")")
    );
    assert!(
        script.contains("id_mask_uniform_writes: numberMetric(metrics, \"id_mask_uniform_writes\")")
    );
    assert!(
        script.contains("id_mask_uniform_bytes: numberMetric(metrics, \"id_mask_uniform_bytes\")")
    );
    assert!(
        script.contains("id_mask_uniform_slots: numberMetric(metrics, \"id_mask_uniform_slots\")")
    );
    assert!(script.contains("spinner_draws: numberMetric(metrics, \"spinner_draws\")"));
    assert!(script.contains("camera_bg_draws: numberMetric(metrics, \"camera_bg_draws\")"));
    assert!(script.contains("render_passes: numberMetric(metrics, \"render_passes\")"));
    assert!(script.contains("clear_passes: numberMetric(metrics, \"clear_passes\")"));
    assert!(script.contains("draw_passes: numberMetric(metrics, \"draw_passes\")"));
    assert!(script.contains("scene3d_passes: numberMetric(metrics, \"scene3d_passes\")"));
    assert!(script.contains(
        "id_mask_field_jump_passes: numberMetric(metrics, \"id_mask_field_jump_passes\")"
    ));
    assert!(script.contains("present_passes: numberMetric(metrics, \"present_passes\")"));
    assert!(script.contains("texture_copies: numberMetric(metrics, \"texture_copies\")"));
    assert!(script.contains("command_buffers: numberMetric(metrics, \"command_buffers\")"));
    assert!(script.contains("buffer_upload_bytes: numberMetric(metrics, \"buffer_upload_bytes\")"));
    assert!(
        script.contains("texture_upload_bytes: numberMetric(metrics, \"texture_upload_bytes\")")
    );
    assert!(script.contains("function resourceMetricFields"));
    assert!(script.contains("function allocationMetricFields"));
    assert!(script.contains("const WASM_FRAME_STAGE_NAMES"));
    assert!(script.contains("const WASM_SUBMIT_STAGE_NAMES"));
    assert!(script.contains("const GPU_TIMESTAMP_STAGE_FIELDS"));
    assert!(script.contains("function frameStageAllocationMetricFields"));
    assert!(script.contains("function submitAllocationMetricFields"));
    assert!(script.contains("function gpuTimestampStageBreakdownSummary"));
    assert!(script.contains("function frameLoopWasmStageSummary"));
    assert!(script.contains("function frameLoopWasmSubmitStageSummary"));
    assert!(script.contains("function assertGpuTimestampStageBreakdown"));
    assert!(script.contains("function assertFrameLoopWasmStageAllocation"));
    assert!(script.contains("function assertFrameLoopWasmSubmitStageAllocation"));
    assert!(script.contains("function wasmAllocationSummary"));
    assert!(script.contains("function assertWasmAllocationAudit"));
    assert!(script.contains("gpu_timestamp_stage_breakdown"));
    assert!(script.contains("wasm_allocation_audit"));
    assert!(script.contains("frame_loop_wasm_allocation_stages"));
    assert!(script.contains("wasm_alloc_count: numberMetric(metrics, key(\"wasm_alloc_count\"))"));
    assert!(
        script.contains("wasm_realloc_count: numberMetric(metrics, key(\"wasm_realloc_count\"))")
    );
    assert!(script.contains(
        "wasm_allocating_frames: numberMetric(metrics, key(\"wasm_allocating_frames\"))"
    ));
    assert!(script.contains("wasm_peak_frame_alloc_bytes"));
    assert!(script.contains("submit_total_alloc_count"));
    assert!(script.contains("\"surface\""));
    assert!(script.contains("\"finish_queue\""));
    assert!(script.contains("\"timestamp_map\""));
    assert!(script.contains("let key = `submit_${name}_`;"));
    assert!(script.contains("fields[`${key}alloc_count`]"));
    assert!(script.contains("let prefix = `wasm_stage_${name}_`;"));
    assert!(script.contains("web.wasm.webgpu.frame_loop_wasm_allocation_stages"));
    assert!(script.contains("web.wasm.webgpu.frame_loop_wasm_submit_allocation_stages"));
    assert!(script.contains("buffer_grows: numberMetric(metrics, key(\"buffer_grows\"))"));
    assert!(script.contains("draw_buffer_grows: numberMetric(metrics, key(\"draw_buffer_grows\"))"));
    assert!(script
        .contains("image_texture_creates: numberMetric(metrics, key(\"image_texture_creates\"))"));
    assert!(script.contains(
        "target_bind_group_creates: numberMetric(metrics, key(\"target_bind_group_creates\"))"
    ));
    assert!(script
        .contains("layer_texture_creates: numberMetric(metrics, key(\"layer_texture_creates\"))"));
    assert!(script.contains(
        "layer_bind_group_creates: numberMetric(metrics, key(\"layer_bind_group_creates\"))"
    ));
    assert!(script
        .contains("scene3d_buffer_grows: numberMetric(metrics, key(\"scene3d_buffer_grows\"))"));
    assert!(script.contains(
        "effect_bind_group_creates: numberMetric(metrics, key(\"effect_bind_group_creates\"))"
    ));
    assert!(script
        .contains("id_mask_buffer_grows: numberMetric(metrics, key(\"id_mask_buffer_grows\"))"));
    assert!(script.contains(
        "image_upload_temp_allocs: numberMetric(metrics, `${prefix}_image_upload_temp_allocs`)"
    ));
    assert!(script.contains(
        "image_upload_scratch_bytes: numberMetric(metrics, `${prefix}_image_upload_scratch_bytes`)"
    ));
    assert!(script.contains("function scratchMetricFields"));
    assert!(script.contains("cpu_scratch_bytes: numberMetric(metrics, key(\"cpu_scratch_bytes\"))"));
    assert!(script.contains("cpu_scratch_grows: numberMetric(metrics, key(\"cpu_scratch_grows\"))"));
    assert!(script.contains(
        "cpu_scratch_growth_bytes: numberMetric(metrics, key(\"cpu_scratch_growth_bytes\"))"
    ));
    assert!(script.contains(
        "cpu_draw_scratch_bytes: numberMetric(metrics, key(\"cpu_draw_scratch_bytes\"))"
    ));
    assert!(script.contains(
        "cpu_scene3d_scratch_grows: numberMetric(metrics, key(\"cpu_scene3d_scratch_grows\"))"
    ));
    assert!(script.contains("cpu_effect_scratch_growth_bytes: numberMetric(metrics, key(\"cpu_effect_scratch_growth_bytes\"))"));
    assert!(script.contains(
        "cpu_id_mask_scratch_grows: numberMetric(metrics, key(\"cpu_id_mask_scratch_grows\"))"
    ));
    assert!(script.contains("cpu_image_upload_scratch_growth_bytes: numberMetric(metrics, key(\"cpu_image_upload_scratch_growth_bytes\"))"));
    assert!(script.contains("cpu_resource_table_scratch_grows: numberMetric(metrics, key(\"cpu_resource_table_scratch_grows\"))"));
    assert!(script.contains("commands_traversed: numberMetric(metrics, key(\"commands_traversed\"))"));
    assert!(script.contains("geometry_bytes_copied: numberMetric(metrics, key(\"geometry_bytes_copied\"))"));
    assert!(script.contains("actual_submissions: numberMetric(metrics, key(\"actual_submissions\"))"));
    assert!(script.contains("gpu_logical_total_bytes: numberMetric(metrics, key(\"gpu_logical_total_bytes\"))"));
    assert!(script.contains("gpu_allocated_total_bytes: numberMetric(metrics, key(\"gpu_allocated_total_bytes\"))"));
    assert!(script.contains("`${prefix}_render_passes`"));
    assert!(script.contains("`${prefix}_clear_passes`"));
    assert!(script.contains("`${prefix}_id_mask_field_jump_passes`"));
    assert!(script.contains("`${prefix}_texture_copies`"));
    assert!(script.contains("`${prefix}_layer_draws`"));
    assert!(script.contains("`${prefix}_layer_cache_hits`"));
    assert!(script.contains("`${prefix}_layer_cache_misses`"));
    assert!(script.contains("`${prefix}_layer_cache_skipped_draws`"));
    assert!(script.contains("`${prefix}_layer_passes`"));
    assert!(script.contains("`${prefix}_scene3d_draws`"));
    assert!(script.contains("`${prefix}_effect_uniform_writes`"));
    assert!(script.contains("`${prefix}_buffer_upload_bytes`"));
    assert!(script.contains("function prefixedBackendCase"));
    assert!(script.contains("upload_summary"));
    assert!(script.contains("Upload Summary"));
    assert!(script.contains("Effect Uniform Summary"));
    assert!(script.contains("current_gpu_timestamp_total_ns"));
    assert!(script.contains("current_gpu_timestamp_passes"));
    assert!(script.contains("glyph_current_gpu_timestamp_total_ns"));
    assert!(script.contains("image_current_gpu_timestamp_total_ns"));
    assert!(script.contains("scene3d_summary"));
    assert!(script.contains("scene3d_stress_summary"));
    assert!(script.contains("Scene3D Summary"));
    assert!(script.contains("Scene3D Stress Summary"));
    assert!(script.contains("expected_layers"));
    assert!(script.contains("expected_damage_rects"));
    assert!(script.contains("expected_image_meshes"));
    assert!(script.contains("expected_nine_slices"));
    assert!(script.contains("expected_sdf_glyphs"));
    assert!(script.contains("expected_camera_bg"));
    assert!(script.contains("expected_markers"));
    assert!(script.contains("expected_image_draws"));
    assert!(script.contains("expected_draw_items"));
    assert!(script.contains("expected_clip_runs"));
    assert!(script.contains("expected_clip_depth"));
    assert!(script.contains("expected_backdrops"));
    assert!(script.contains("effect-uniform WebGPU current row"));
    assert!(script.contains("wasm_memory_total_growth_bytes"));
    assert!(script.contains("wasm_memory_max_growth_bytes"));
    assert!(script.contains("wasm_memory_growth_labels"));
    assert!(script.contains("wasm_memory_growth_bytes"));
    assert!(script.contains("summary.wasm_memory_total_growth_bytes !== 0"));
    assert!(script.contains("summary.wasm_memory_max_growth_bytes !== 0"));
    assert!(script.contains("summary.wasm_memory_growth_labels.length !== 0"));
    assert!(script.contains("mark.wasm_memory_growth_bytes !== 0"));
    assert!(script.contains("js_heap_sample_supported_count"));
    assert!(script.contains("js_heap_gc_available_count"));
    assert!(script.contains("js_heap_total_growth_bytes"));
    assert!(script.contains("js_heap_max_growth_bytes"));
    assert!(script.contains("js_heap_growth_labels"));
    assert!(script.contains("mark.js_heap_before_bytes <= 0.0"));
    assert!(script.contains("mark.js_heap_growth_bytes < 0.0"));
    assert!(script.contains("Chrome JS heap sampling and exposed GC"));
    assert!(script.contains("web benchmark report failed during"));
    assert!(script.contains("glyph-run WebGPU current row"));
    assert!(script.contains("function assertWebReportContract"));
    assert!(script.contains("GPU Stage Attribution"));
    assert!(script.contains("timestampMetricFields"));
    assert!(script.contains("timestamp-query-collected"));
    assert!(script.contains("effect-uniform legacy row must stay retired"));
    assert!(script.contains("adapter.features+renderer.timestamp_writes"));
    assert!(script.contains("pass-family total"));
    assert!(script.contains("post-warmup resource creation"));
    assert!(script.contains("post-warmup CPU scratch growth"));
    assert!(!script.contains("function pacingMetricFields"));
    assert!(script.contains("function rawPacingFields"));
    assert!(script.contains("web.wasm.webgpu.raf_frame_loop"));
    assert!(script.contains("refresh_mode: \"unpaced-tight-loop\""));
    assert!(script.contains("const RAF_CPU_STAGE_NAMES"));
    assert!(script.contains("instrumentation_enabled_ms"));
    assert!(script.contains("queue_pending_final !== 0"));
    assert!(script.contains("submissions_per_raf !== 1"));
    assert!(script.contains("--force-device-scale-factor=${args.dpr}"));
    assert!(script.contains("\"Cross-Origin-Opener-Policy\": \"same-origin\""));
    assert!(script.contains("\"Cross-Origin-Embedder-Policy\": \"require-corp\""));
    assert!(script.contains("--self-test-measurement"));
    assert!(script.contains("--report-only requires --raw-report"));
    assert!(script.contains("N displayed frames did not produce N raw frame and stage samples"));
}

#[test]
fn c15_atlas_adapter_runs_real_chrome_and_persists_selected_samples() {
    let script = include_str!("../../../../scripts/run_webgpu_atlas_c15.mjs");
    assert!(script.contains("bench_webgpu_atlas_c15"));
    assert!(script.contains("--enable-unsafe-webgpu"));
    assert!(script.contains("--use-angle=metal"));
    assert!(script.contains("CHROME_ARCH"));
    assert!(script.contains("warmups: [warmup], samples: [sample], metrics"));
    assert!(script.contains("writeFileSync(output, json)"));
}

#[test]
fn c16_geometry_adapter_covers_compact_and_fallback_streams() {
    let host = include_str!("../src/lib.rs");
    let script = include_str!("../../../../scripts/run_webgpu_geometry_c16.mjs");
    assert!(host.contains("pub async fn bench_webgpu_geometry_c16"));
    assert!(host.contains("const WEBGPU_GEOMETRY_QUADS: usize = 10_000"));
    assert!(host.contains("const WEBGPU_GEOMETRY_LARGE_VERTICES: usize = 70_002"));
    assert!(host.contains("glyphs: webgpu_geometry_glyphs(glyph_atlas)?"));
    assert!(host.contains("images: webgpu_geometry_images(image)"));
    assert!(host.contains("large_mesh: webgpu_geometry_large_mesh()"));
    assert!(script.contains("bench_webgpu_geometry_c16"));
    assert!(script.contains("warmups: [warmup], samples: [sample], metrics"));
    assert!(script.contains("--use-angle=metal"));
}

#[test]
fn c19_target_adapter_covers_construction_resize_and_first_declared_use() {
    let host = include_str!("../src/lib.rs");
    let script = include_str!("../../../../scripts/run_webgpu_targets_c19.mjs");

    assert!(host.contains("pub async fn bench_webgpu_targets_c19"));
    assert!(host.contains("fn c19_direct_frame"));
    assert!(host.contains("fn c19_backdrop_frame"));
    assert!(host.contains("fn c19_scene3d_frame"));
    assert!(host.contains("resize_direct_target_creates="));
    assert!(host.contains("resize_scene3d_target_creates="));
    assert!(host.contains("backdrop_prewarm_target_creates="));
    assert!(host.contains("scene3d_prewarm_target_creates="));
    assert!(host.contains("backdrop_ready_ms"));
    assert!(host.contains("scene3d_ready_ms"));
    assert!(host.contains("direct_transient_target_bytes="));
    assert!(host.contains("backdrop_transient_target_bytes="));
    assert!(host.contains("scene3d_depth_target_bytes="));
    assert!(script.contains("GPUDevice.prototype.createTexture"));
    assert!(script.contains("GPUDevice.prototype.createBindGroup"));
    assert!(script.contains("construction_texture_creates"));
    assert!(script.contains("construction_bind_group_creates"));
    assert!(script.contains("--force-device-scale-factor=2"));
    assert!(script.contains("warmups, samples, metrics"));
    assert!(script.contains("writeFileSync(output, json)"));
}

#[test]
fn c20_web_scheduler_coalesces_invalidations_and_caches_canvas_geometry() {
    let host = include_str!("../src/lib.rs");
    let script = include_str!("../../../../scripts/run_web_scheduler_c20.mjs");
    let frame = source_fn_slice(host, "fn frame_at_inner(", "fn mark_frame_dirty");
    let frame_event = source_fn_slice(
        host,
        "fn install_frame_event_listener(",
        "fn route_key(",
    );
    let pointer = source_fn_slice(host, "fn install_pointer_listener(", "fn install_wheel_listener(");

    assert!(host.contains("struct CanvasMetrics"));
    assert!(host.contains("ResizeObserver::new"));
    assert!(host.contains("MutationObserver::new"));
    assert!(host.contains("install_frame_event_listener(state, window_target, \"scroll\", true, true)"));
    assert!(frame.contains("self.refresh_canvas_metrics()?"));
    assert!(!frame.contains("get_bounding_client_rect"));
    assert!(pointer.contains("state.refresh_canvas_metrics()"));
    assert!(pointer.contains("state.canvas_metrics"));
    assert!(pointer.contains("state.pointer_anticipation = true"));
    assert!(!pointer.contains("get_bounding_client_rect"));
    assert!(frame_event.contains("state.mark_canvas_metrics_dirty()"));
    assert!(frame_event.contains("request_next_frame(&state_for_event)"));
    assert!(!frame_event.contains("frame_at(perf_now())"));
    assert!(host.contains("last_timestamp_ms: f64"));
    assert!(host.contains("frame_time_remainder_ms: f64"));
    assert!(!host.contains("IDLE_SETTLE_FRAMES"));
    assert!(!host.contains("settle_frames_remaining"));
    assert!(host.contains("let needs_frame = handled || ime_focused && down"));
    assert!(host.contains("pub fn web_scheduler_metrics"));
    assert!(script.contains("const discreteSampleCount = 100"));
    assert!(script.contains("const pointerSampleCount = 240"));
    assert!(script.contains("app.set_scene(4)"));
    assert!(script.contains("key: \"ArrowRight\""));
    assert!(script.contains("pointer240hz"));
    assert!(script.contains("window.dispatchEvent(new Event(\"resize\"))"));
    assert!(script.contains("window.dispatchEvent(new Event(\"oxide-redraw\"))"));
    assert!(script.contains("canvas.style.width = \"calc(100vw - 32px)\""));
    assert!(script.contains("missed_frames"));
    assert!(script.contains("event_to_visible_ms"));
}


#[test]
fn c33_id_mask_cache_probe_covers_key_cases_lru_and_valid_gpu_samples()
{
   let source = include_str!("../src/lib.rs");
   let html = include_str!("../../www/index.html");
   let script = include_str!("../../../../scripts/check_webgpu_browser_golden.mjs");

   for case in [
      "static",
      "style",
      "viewport",
      "projection",
      "content",
      "one_entry_multi",
      "lru_multi",
   ]
   {
      assert!(source.contains(case), "missing C33 ID-mask case {case}");
   }
   assert!(source.contains("expected {expected} GPU samples"));
   assert!(source.contains("sample.id_mask_raster_ns"));
   assert!(source.contains("sample.id_mask_field_seed_ns"));
   assert!(source.contains("sample.id_mask_field_jump_ns"));
   assert!(source.contains("sample.id_mask_compositor_ns"));
   assert!(source.contains("purge_id_mask_field_cache_for_memory_pressure"));
   assert!(source.contains("purge_id_mask_field_cache_for_device_loss_for_benchmark"));
   assert!(source.contains("id_mask_cache_hits={}"));
   assert!(source.contains("id_mask_cache_resident_bytes={}"));
   assert!(html.contains("id_mask_cache_only"));
   assert!(html.contains("runIdMaskCacheRafHarness"));
   assert!(html.contains("submissions_per_raf: 1"));
   assert!(html.contains("id_mask_cache_raf_c33: window.oxideWebGpuIdMaskCacheRafC33"));
   assert!(html.contains("bench_webgpu_id_mask_cache_c33"));
   assert!(html.contains("id_mask_cache_c33: window.oxideWebGpuIdMaskCacheC33"));
   assert!(script.contains("--id-mask-cache-only"));
   assert!(script.contains("--id-mask-cache-raf-only"));
   assert!(script.contains("id_mask_cache_raf_frames"));
   assert!(script.contains("id_mask_cache_only"));
}

#[test]
fn c35_id_mask_probe_reports_selected_field_representation_and_exact_bytes()
{
   let host = include_str!("../src/lib.rs");
   let renderer = include_str!("../../../../crates/renderer-web/src/wasm/webgpu.rs");
   let html = include_str!("../../www/index.html");
   let script = include_str!("../../../../scripts/check_webgpu_browser_golden.mjs");

   assert!(host.contains("let one_entry_budget = renderer"));
   assert!(host.contains(".id_mask_target_bytes_per_pixel()"));
   assert!(!host.contains("512 * 512 * 34"));
   for field in [
      "\\\"packed_fields\\\":{}",
      "\\\"field_logical_bytes\\\":{}",
      "\\\"wide_field_logical_bytes\\\":{}",
   ]
   {
      assert!(host.contains(field), "missing C35 ID-mask proof field {field}");
   }
   assert!(renderer.contains("pub fn id_mask_target_bytes_per_pixel(&self) -> u64"));
   assert!(renderer.contains("pub fn id_mask_packed_fields_supported(&self) -> bool"));
   assert!(renderer.contains("2 * color_texture_bytes_per_pixel(ID_MASK_PACKED_FIELD_FORMAT)"));
   assert!(renderer.contains("4 * color_texture_bytes_per_pixel(ID_MASK_WIDE_FIELD_FORMAT)"));
   assert!(host.contains("pub async fn read_webgpu_id_mask_field_matrix"));
   assert!(host.contains("webgpu_single_seed_id_mask_frame"));
   for dimensions in [
      "(256_usize, 256_usize)",
      "(512, 512)",
      "(1024, 1024)",
      "(2048, 2048)",
      "(257, 509)",
      "(2048, 257)",
      "(511, 1024)",
   ]
   {
      assert!(host.contains(dimensions), "missing C35 matrix dimensions {dimensions}");
   }
   for mismatch in [
      "city_mismatches",
      "neighborhood_mismatches",
      "city_field_mismatches",
      "seam_field_mismatches",
   ]
   {
      assert!(host.contains(mismatch), "missing C35 matrix counter {mismatch}");
   }
   assert!(html.contains("id_mask_matrix_only"));
   assert!(html.contains("read_webgpu_id_mask_field_matrix"));
   assert!(script.contains("--id-mask-matrix-out"));
   assert!(script.contains("browser report omitted WebGPU ID-mask field matrix"));
}
