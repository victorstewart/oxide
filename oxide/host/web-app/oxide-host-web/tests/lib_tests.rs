use oxide_host_web::generate_checker_rgba;
use std::io::Cursor;

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

fn report_case_slice<'a>(report: &'a str, id: &str) -> &'a str {
    let cases_marker = "\"cases\": [";
    let cases_start =
        report.find(cases_marker).unwrap_or_else(|| panic!("missing web report cases array"))
            + cases_marker.len();
    let marker = format!("\"id\": \"{id}\"");
    let start = report[cases_start..]
        .find(&marker)
        .unwrap_or_else(|| panic!("missing web report case {id}"))
        + cases_start;
    let tail = &report[start..];
    let end = tail.find("\n    }").unwrap_or(tail.len());
    &tail[..end]
}

fn report_section_slice<'a>(report: &'a str, section: &str) -> &'a str {
    let marker = format!("\"{section}\": {{");
    let start =
        report.find(&marker).unwrap_or_else(|| panic!("missing web report section {section}"));
    let tail = &report[start..];
    let end = tail.find("\n  }").unwrap_or(tail.len());
    &tail[..end]
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

fn report_f64(section: &str, key: &str) -> f64 {
    let marker = format!("\"{key}\": ");
    let start =
        section.find(&marker).unwrap_or_else(|| panic!("missing numeric report field {key}"))
            + marker.len();
    let rest = &section[start..];
    let end = rest.find(|ch: char| ch == ',' || ch == '\n' || ch == '}').unwrap_or(rest.len());
    rest[..end]
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("invalid numeric report field {key}"))
}

fn report_u64(section: &str, key: &str) -> u64 {
    report_f64(section, key) as u64
}

fn report_pass_family_total(section: &str) -> f64 {
    report_f64(section, "clear_passes")
        + report_f64(section, "draw_passes")
        + report_f64(section, "scene3d_passes")
        + report_f64(section, "scene3d_overlay_passes")
        + report_f64(section, "id_mask_raster_passes")
        + report_f64(section, "id_mask_field_seed_passes")
        + report_f64(section, "id_mask_field_jump_passes")
        + report_f64(section, "id_mask_compositor_passes")
        + report_f64(section, "present_passes")
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
    assert!(html.contains("bench_webgpu_scene3d_ab"));
    assert!(html.contains("bench_webgpu_mixed_matrix"));
    assert!(html.contains("bench_webgpu_layer_effects_matrix"));
    assert!(html.contains("bench_webgpu_clean_layer_ab"));
    assert!(html.contains("bench_webgpu_command_family_matrix"));
    assert!(html.contains("bench_webgpu_glyph_run_current"));
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
    assert!(html.contains("scene3d_ab"));
    assert!(html.contains("mixed_matrix"));
    assert!(html.contains("layer_effects_matrix"));
    assert!(html.contains("clean_layer_ab"));
    assert!(html.contains("command_family_matrix"));
    assert!(html.contains("glyph_run_current"));
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
    assert!(source.contains("pub async fn bench_webgpu_scene3d_ab"));
    assert!(source.contains("pub async fn bench_webgpu_mixed_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_layer_effects_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_clean_layer_ab"));
    assert!(source.contains("pub async fn bench_webgpu_command_family_matrix"));
    assert!(source.contains("pub async fn bench_webgpu_glyph_run_current"));
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
    assert!(source.contains("pub async fn finish_raf_gpu_timestamp_capture"));
    assert!(source.contains("timestamp_samples_json"));
    assert!(!source.contains("{backend_stats}{pacing}{allocations}"));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_ab"));
    assert!(source.contains("pub async fn bench_webgpu_id_mask_current"));
    assert!(source.contains("pub async fn bench_webgpu_upload_current"));
    assert!(!source.contains("pub async fn bench_webgpu_upload_ab"));
    assert!(source.contains("pub async fn bench_webgpu_effect_uniform_ab"));
    assert!(source.contains("pub async fn bench_webgpu_backdrop_batch_current"));
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
    for pixel in rgba.chunks_exact(4) {
        let r = pixel[0];
        let g = pixel[1];
        let b = pixel[2];
        let a = pixel[3];
        assert_eq!(a, 255);
        if b > 180 && r < 120 && g > 80 {
            blue_pixels += 1;
        }
        if r < 16 && g < 16 && b < 16 {
            dark_pixels += 1;
        }
        if r > 235 && g > 235 && b > 235 {
            background_pixels += 1;
        }
    }

    assert!(blue_pixels > 4000, "WebGPU golden is missing the blue control surfaces");
    assert!(dark_pixels > 3000, "WebGPU golden is missing the captured page bounds");
    assert!(background_pixels > 20000, "WebGPU golden is missing the light scene background");
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
    assert!(script.contains("upload_current"));
    assert!(script.contains("backdrop_batch_current"));
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
fn committed_webgpu_browser_baseline_persists_nonzero_id_mask_current_row() {
    let report = include_str!("../../../../benchmarks/web/latest.json");
    assert!(report.contains("\"version\": 5"));
    assert!(report.contains("\"suite\": \"web-wasm\""));
    assert!(report.contains("\"webgpu\": \"webgpu=device-ok\""));
    assert!(report.contains("\"webgpu_timing\": \"timestamp_query="));
    assert!(report.contains("\"gpu_stage_attribution\": {"));
    assert!(report.contains("\"status\": \"timestamp-query-collected\""));
    assert!(report.contains("\"source\": \"adapter.features+renderer.timestamp_writes\""));
    assert!(report.contains("\"backend\": \"webgpu\""));
    assert!(report.contains("\"capture_target\": \"app\""));
    assert!(report.contains("\"browser_startup\": {"));
    assert!(report.contains("\"browser_trace\": {"));
    assert!(report.contains("\"warm_resource_churn\": {"));
    assert!(report.contains("\"wasm_allocation_audit\": {"));

    let frame = report_case_slice(report, "web.wasm.webgpu.frame_loop");
    let current = report_case_slice(report, "web.wasm.webgpu.id_mask_compositor.current");
    let glyph_current =
        report_case_slice(report, "web.wasm.webgpu.glyph_atlas_upload.current_dirty");
    let image_current = report_case_slice(report, "web.wasm.webgpu.image_upload.current_dirty");
    let effect_current =
        report_case_slice(report, "web.wasm.webgpu.effect_uniform.current_batched");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.effect_uniform.legacy_write_each\""));
    let backdrop_batch_current =
        report_case_slice(report, "web.wasm.webgpu.backdrop_batch.current_coalesced");
    let scene3d_reused = report_case_slice(report, "web.wasm.webgpu.scene3d.reused_mesh");
    let scene3d_recreate = report_case_slice(report, "web.wasm.webgpu.scene3d.recreate_mesh");
    let scene3d_stress_reused =
        report_case_slice(report, "web.wasm.webgpu.scene3d.stress_reused_mesh");
    let scene3d_stress_recreate =
        report_case_slice(report, "web.wasm.webgpu.scene3d.stress_recreate_mesh");
    let mixed = report_case_slice(report, "web.wasm.webgpu.mixed_text_image_effects");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched\""));
    let layer_effects = report_case_slice(report, "web.wasm.webgpu.layer_damage_effects");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched\""));
    let clean_layer = report_case_slice(report, "web.wasm.webgpu.clean_layer.clean_reuse");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.clean_layer.dirty_rerender\""));
    let command_family = report_case_slice(report, "web.wasm.webgpu.command_family_matrix");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.command_family_matrix.legacy_rebind\""));
    let glyph_run_current = report_case_slice(report, "web.wasm.webgpu.glyph_run.current");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.glyph_run.legacy_rebind\""));
    let neon_marker_current = report_case_slice(report, "web.wasm.webgpu.neon_marker.current");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.neon_marker.legacy_rebind\""));
    let direct_surface_current =
        report_case_slice(report, "web.wasm.webgpu.direct_surface.current");
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.direct_surface.legacy_scene_present\""));

    assert!(
        report_f64(frame, "p50_ms") > 0.0,
        "frame-loop timing must be real, not virtual-time zero"
    );
    assert_eq!(report_f64(frame, "missed_frame_ratio_120hz"), 0.0);
    assert_eq!(report_f64(frame, "hitch_ratio_120hz"), 0.0);
    assert!(report_f64(frame, "solid_tris") > 0.0);
    assert!(report_f64(frame, "image_draws") >= 0.0);
    assert!(report_f64(frame, "glyph_quads") > 0.0);
    assert!(report_f64(frame, "clip_depth_peak") >= 0.0);
    assert!(report_f64(frame, "damage_rects") >= 0.0);
    assert!(report_f64(frame, "layer_draws") >= 0.0);
    assert!(report_f64(frame, "layer_cache_hits") >= 0.0);
    assert!(report_f64(frame, "layer_cache_misses") >= 0.0);
    assert!(report_f64(frame, "layer_cache_skipped_draws") >= 0.0);
    assert!(report_f64(frame, "layer_passes") >= 0.0);
    assert!(report_f64(frame, "scene3d_draws") >= 0.0);
    assert!(report_f64(frame, "id_mask_draws") >= 0.0);
    assert!(report_f64(frame, "backdrop_draws") >= 0.0);
    assert!(report_f64(frame, "visual_effect_draws") >= 0.0);
    assert!(report_f64(frame, "spinner_draws") >= 0.0);
    assert!(report_f64(frame, "camera_bg_draws") >= 0.0);
    assert!(report_f64(frame, "render_passes") > 0.0);
    assert_eq!(report_pass_family_total(frame), report_f64(frame, "render_passes"));
    assert!(report_f64(frame, "draw_passes") > 0.0);
    assert!(report_f64(frame, "texture_copies") >= 0.0);
    assert!(report_f64(frame, "command_buffers") > 0.0);
    assert_eq!(report_u64(frame, "gpu_timestamp_query_supported"), 1);
    assert!(report_f64(frame, "gpu_timestamp_passes") > 0.0);
    assert!(report_f64(frame, "gpu_timestamp_total_ns") >= 0.0);
    assert!(report_f64(frame, "gpu_timestamp_readback_interval") >= 1.0);
    assert!(report_f64(frame, "buffer_upload_bytes") > 0.0);
    assert!(report_f64(frame, "texture_upload_bytes") >= 0.0);
    assert!(report_f64(frame, "buffer_grows") >= 0.0);
    assert!(report_f64(frame, "texture_creates") >= 0.0);
    assert!(report_f64(frame, "bind_group_creates") >= 0.0);
    assert!(report_f64(frame, "layer_texture_creates") >= 0.0);
    assert!(report_f64(frame, "layer_bind_group_creates") >= 0.0);
    assert!(report_f64(frame, "pipeline_creates") >= 0.0);
    assert_eq!(report_u64(frame, "sampler_creates"), 0);
    assert!(report_f64(frame, "mesh3d_creates") >= 0.0);
    assert!(report_f64(frame, "wasm_alloc_count") >= 0.0);
    assert!(report_f64(frame, "wasm_alloc_bytes") >= 0.0);
    assert!(report_f64(frame, "wasm_realloc_count") >= 0.0);
    assert!(report_f64(frame, "wasm_realloc_grow_bytes") >= 0.0);
    assert!(report_f64(frame, "wasm_allocating_frames") >= 0.0);
    assert!(report_f64(frame, "wasm_peak_frame_alloc_bytes") >= 0.0);
    assert_eq!(report_u64(frame, "draw_buffer_grows"), 0);
    assert_eq!(report_u64(frame, "image_texture_creates"), 0);
    assert_eq!(report_u64(frame, "image_bind_group_creates"), 0);
    assert_eq!(report_u64(frame, "target_texture_creates"), 0);
    assert_eq!(report_u64(frame, "target_bind_group_creates"), 0);
    assert_eq!(report_u64(frame, "scene3d_buffer_grows"), 0);
    assert_eq!(report_u64(frame, "scene3d_bind_group_creates"), 0);
    assert_eq!(report_u64(frame, "effect_buffer_grows"), 0);
    assert_eq!(report_u64(frame, "effect_bind_group_creates"), 0);
    assert_eq!(report_u64(frame, "id_mask_texture_creates"), 0);
    assert_eq!(report_u64(frame, "id_mask_buffer_grows"), 0);
    assert_eq!(report_u64(frame, "id_mask_bind_group_creates"), 0);
    assert!(report_f64(frame, "cpu_scratch_bytes") > 0.0);
    assert_eq!(report_u64(frame, "cpu_scratch_grows"), 0);
    assert_eq!(report_u64(frame, "cpu_scratch_growth_bytes"), 0);
    assert!(report_f64(frame, "cpu_draw_scratch_bytes") > 0.0);
    assert_eq!(report_u64(frame, "cpu_draw_scratch_grows"), 0);
    assert_eq!(report_u64(frame, "cpu_draw_scratch_growth_bytes"), 0);
    assert!(report_f64(frame, "cpu_resource_table_scratch_bytes") > 0.0);
    assert_eq!(report_u64(frame, "cpu_resource_table_scratch_grows"), 0);
    assert_eq!(report_u64(frame, "cpu_resource_table_scratch_growth_bytes"), 0);
    assert!(
        report_f64(current, "p50_ms") > 0.0,
        "current WebGPU ID-mask row must have nonzero timing"
    );
    assert!(report_f64(current, "p99_ms") >= report_f64(current, "p50_ms"));
    assert_eq!(report_f64(current, "missed_frame_ratio_120hz"), 0.0);
    assert!(report_f64(current, "draws") > 0.0);
    assert!(report_f64(current, "id_mask_draws") > 0.0);
    assert!(report_f64(current, "id_mask_raster_passes") > 0.0);
    assert!(report_f64(current, "id_mask_field_jump_passes") > 0.0);
    assert!(report_f64(current, "id_mask_compositor_passes") > 0.0);
    assert_eq!(report_pass_family_total(current), report_f64(current, "render_passes"));
    assert!(report_f64(current, "layer_draws") >= 0.0);
    assert!(report_f64(current, "render_passes") > 0.0);
    assert!(report_f64(current, "command_buffers") > 0.0);
    assert_eq!(report_u64(current, "gpu_timestamp_query_supported"), 1);
    assert!(report_f64(current, "gpu_timestamp_passes") > 0.0);
    assert!(report_f64(current, "gpu_timestamp_id_mask_field_jump_ns") >= 0.0);
    assert!(report_f64(current, "buffer_upload_bytes") > 0.0);
    assert_eq!(report_u64(current, "sampler_creates"), 0);
    assert_eq!(report_u64(current, "cpu_scratch_grows"), 0);
    assert_eq!(report_u64(current, "cpu_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(current, "vertices"), 9600);
    assert_eq!(report_u64(current, "vertex_bytes"), 307200);
    assert!(report_f64(scene3d_reused, "p50_ms") > 0.0);
    assert!(report_f64(scene3d_recreate, "p50_ms") > 0.0);
    assert!(report_f64(scene3d_reused, "scene3d_draws") > 0.0);
    assert!(report_f64(scene3d_reused, "scene3d_passes") > 0.0);
    assert_eq!(
        report_pass_family_total(scene3d_reused),
        report_f64(scene3d_reused, "render_passes")
    );
    assert!(report_f64(scene3d_recreate, "scene3d_draws") > 0.0);
    assert_eq!(report_u64(scene3d_reused, "mesh3d_creates"), 0);
    assert!(report_u64(scene3d_recreate, "mesh3d_creates") > 0);
    assert_eq!(report_u64(scene3d_reused, "buffer_grows"), 0);
    assert!(report_u64(scene3d_recreate, "buffer_grows") > 0);
    assert_eq!(report_u64(scene3d_reused, "cpu_scratch_grows"), 0);
    assert_eq!(report_u64(scene3d_reused, "cpu_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(scene3d_reused, "meshes"), 2);
    assert_eq!(report_u64(scene3d_reused, "instances"), 2);
    assert!(report_f64(scene3d_stress_reused, "p50_ms") > 0.0);
    assert!(report_f64(scene3d_stress_recreate, "p50_ms") > 0.0);
    assert!(report_u64(scene3d_stress_reused, "scene3d_draws") >= 64);
    assert!(report_u64(scene3d_stress_recreate, "scene3d_draws") >= 64);
    assert_eq!(report_u64(scene3d_stress_reused, "mesh3d_creates"), 0);
    assert!(report_u64(scene3d_stress_recreate, "mesh3d_creates") > 0);
    assert_eq!(report_u64(scene3d_stress_reused, "buffer_grows"), 0);
    assert!(report_u64(scene3d_stress_recreate, "buffer_grows") > 0);
    assert_eq!(report_u64(scene3d_stress_reused, "cpu_scratch_grows"), 0);
    assert_eq!(report_u64(scene3d_stress_reused, "cpu_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(scene3d_stress_reused, "meshes"), 2);
    assert!(report_u64(scene3d_stress_reused, "instances") >= 64);
    assert!(report_f64(glyph_current, "p50_ms") > 0.0);
    assert!(report_f64(glyph_current, "glyph_quads") > 0.0);
    assert_eq!(
        report_f64(glyph_current, "gpu_timestamp_passes"),
        report_f64(glyph_current, "render_passes")
    );
    assert!(report_f64(glyph_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(report_f64(glyph_current, "buffer_upload_bytes") > 0.0);
    assert_eq!(report_u64(glyph_current, "atlas_width"), 1024);
    assert_eq!(report_u64(glyph_current, "dirty_width"), 64);
    assert!(report_f64(image_current, "p50_ms") > 0.0);
    assert!(report_f64(image_current, "image_draws") > 0.0);
    assert_eq!(
        report_f64(image_current, "gpu_timestamp_passes"),
        report_f64(image_current, "render_passes")
    );
    assert!(report_f64(image_current, "gpu_timestamp_total_ns") > 0.0);
    assert_eq!(report_u64(image_current, "image_width"), 256);
    assert_eq!(report_u64(image_current, "dirty_width"), 64);
    assert!(
        report_f64(effect_current, "backdrop_draws")
            >= report_f64(effect_current, "expected_backdrops")
    );
    assert_eq!(report_u64(effect_current, "effect_uniform_writes"), 1);
    assert!(report_u64(effect_current, "effect_uniform_bytes") > 0);
    assert_eq!(
        report_f64(effect_current, "effect_uniform_slots"),
        report_f64(effect_current, "expected_backdrops")
    );
    assert_eq!(
        report_f64(effect_current, "gpu_timestamp_passes"),
        report_f64(effect_current, "render_passes")
    );
    assert!(report_f64(effect_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(
        report_f64(backdrop_batch_current, "backdrop_draws")
            >= report_f64(backdrop_batch_current, "expected_backdrops")
    );
    assert_eq!(report_u64(backdrop_batch_current, "effect_uniform_writes"), 1);
    assert_eq!(
        report_f64(backdrop_batch_current, "effect_uniform_slots"),
        report_f64(backdrop_batch_current, "expected_backdrops")
    );
    assert_eq!(report_u64(backdrop_batch_current, "texture_copies"), 1);
    assert_eq!(report_u64(backdrop_batch_current, "render_passes"), 4);
    assert_eq!(
        report_f64(backdrop_batch_current, "gpu_timestamp_passes"),
        report_f64(backdrop_batch_current, "render_passes")
    );
    assert!(report_f64(mixed, "image_draws") > 0.0);
    assert!(report_f64(mixed, "glyph_quads") > 0.0);
    assert!(report_f64(mixed, "layer_draws") > 0.0);
    assert!(report_f64(mixed, "clip_depth_peak") > 0.0);
    assert!(report_f64(mixed, "backdrop_draws") > 0.0);
    assert!(report_f64(mixed, "visual_effect_draws") > 0.0);
    assert!(report_f64(mixed, "spinner_draws") > 0.0);
    assert!(report_f64(mixed, "render_passes") > 1.0);
    assert!(report_f64(mixed, "texture_copies") > 0.0);
    assert_eq!(report_pass_family_total(mixed), report_f64(mixed, "render_passes"));
    assert!(report_f64(mixed, "damage_rects") > 0.0);
    assert!(report_f64(mixed, "image_draws") >= report_f64(mixed, "image_tiles"));
    assert!(report_f64(mixed, "draw_pipeline_binds") > 0.0);
    assert!(report_f64(mixed, "draw_bind_group_binds") > 0.0);
    assert!(report_f64(mixed, "draw_scissor_sets") > 0.0);
    assert!(report_f64(mixed, "effect_uniform_writes") > 0.0);
    assert_eq!(report_f64(mixed, "gpu_timestamp_passes"), report_f64(mixed, "render_passes"));
    assert!(report_f64(layer_effects, "image_draws") > 0.0);
    assert!(report_f64(layer_effects, "glyph_quads") > 0.0);
    assert!(
        report_f64(layer_effects, "layer_draws") >= report_f64(layer_effects, "expected_layers")
    );
    assert!(report_f64(layer_effects, "backdrop_draws") > 0.0);
    assert!(report_f64(layer_effects, "visual_effect_draws") > 0.0);
    assert!(report_f64(layer_effects, "spinner_draws") > 0.0);
    assert!(report_f64(layer_effects, "clip_depth_peak") > 0.0);
    assert!(report_f64(layer_effects, "texture_copies") > 0.0);
    assert!(
        report_f64(layer_effects, "damage_rects")
            >= report_f64(layer_effects, "expected_damage_rects")
    );
    assert!(report_f64(layer_effects, "image_draws") >= report_f64(layer_effects, "image_tiles"));
    assert!(report_f64(layer_effects, "draw_pipeline_binds") > 0.0);
    assert!(report_f64(layer_effects, "draw_bind_group_binds") > 0.0);
    assert!(report_f64(layer_effects, "draw_scissor_sets") > 0.0);
    assert!(report_f64(layer_effects, "effect_uniform_writes") > 0.0);
    assert!(report_f64(layer_effects, "render_passes") > 0.0);
    assert_eq!(report_pass_family_total(layer_effects), report_f64(layer_effects, "render_passes"));
    assert_eq!(
        report_f64(layer_effects, "gpu_timestamp_passes"),
        report_f64(layer_effects, "render_passes")
    );
    assert_eq!(report_f64(clean_layer, "layer_cache_hits"), 1.0);
    assert_eq!(report_f64(clean_layer, "layer_cache_misses"), 0.0);
    assert!(
        report_f64(clean_layer, "layer_cache_skipped_draws")
            > report_f64(clean_layer, "draw_items")
    );
    assert_eq!(report_f64(clean_layer, "layer_passes"), 0.0);
    assert_eq!(report_pass_family_total(clean_layer), report_f64(clean_layer, "render_passes"));
    assert_eq!(
        report_f64(clean_layer, "gpu_timestamp_passes"),
        report_f64(clean_layer, "render_passes")
    );
    assert!(
        report_f64(command_family, "image_mesh_draws")
            >= report_f64(command_family, "expected_image_meshes")
    );
    assert!(
        report_f64(command_family, "nine_slice_draws")
            >= report_f64(command_family, "expected_nine_slices")
    );
    assert!(
        report_f64(command_family, "sdf_glyph_quads")
            >= report_f64(command_family, "expected_sdf_glyphs")
    );
    assert_eq!(report_f64(command_family, "expected_camera_bg"), 0.0);
    assert_eq!(report_f64(command_family, "camera_bg_draws"), 0.0);
    assert!(report_f64(command_family, "image_draws") >= 10.0);
    assert_eq!(
        report_f64(command_family, "image_mesh_draws"),
        report_f64(command_family, "expected_image_meshes")
    );
    assert_eq!(
        report_f64(command_family, "nine_slice_draws"),
        report_f64(command_family, "expected_nine_slices")
    );
    assert_eq!(
        report_f64(command_family, "sdf_glyph_quads"),
        report_f64(command_family, "expected_sdf_glyphs")
    );
    assert_eq!(
        report_pass_family_total(command_family),
        report_f64(command_family, "render_passes")
    );
    assert_eq!(
        report_f64(command_family, "gpu_timestamp_passes"),
        report_f64(command_family, "render_passes")
    );
    assert_eq!(report_u64(glyph_run_current, "expected_glyph_runs"), 64);
    assert_eq!(report_u64(glyph_run_current, "expected_glyphs_per_run"), 8);
    assert_eq!(report_u64(glyph_run_current, "expected_glyph_quads"), 512);
    assert_eq!(report_u64(glyph_run_current, "expected_sdf_runs"), 32);
    assert_eq!(report_u64(glyph_run_current, "expected_sdf_glyph_quads"), 256);
    assert_eq!(report_u64(glyph_run_current, "expected_draw_items"), 65);
    assert_eq!(
        report_f64(glyph_run_current, "draw_items"),
        report_f64(glyph_run_current, "expected_draw_items")
    );
    assert_eq!(
        report_f64(glyph_run_current, "glyph_quads"),
        report_f64(glyph_run_current, "expected_glyph_quads")
    );
    assert_eq!(
        report_f64(glyph_run_current, "sdf_glyph_quads"),
        report_f64(glyph_run_current, "expected_sdf_glyph_quads")
    );
    assert_eq!(report_f64(glyph_run_current, "render_passes"), 1.0);
    assert_eq!(report_f64(glyph_run_current, "draw_passes"), 1.0);
    assert!(report_f64(glyph_run_current, "draw_pipeline_binds") > 0.0);
    assert!(report_f64(glyph_run_current, "draw_bind_group_binds") > 0.0);
    assert!(report_f64(glyph_run_current, "draw_scissor_sets") > 0.0);
    assert_eq!(
        report_f64(glyph_run_current, "gpu_timestamp_passes"),
        report_f64(glyph_run_current, "render_passes")
   );
   assert_eq!(report_u64(neon_marker_current, "expected_markers"), 64);
   assert_eq!(report_u64(neon_marker_current, "expected_draw_items"), 192);
   assert_eq!(
       report_f64(neon_marker_current, "draw_items"),
       report_f64(neon_marker_current, "expected_draw_items")
   );
   assert!(report_f64(neon_marker_current, "solid_tris") > 0.0);
   assert_eq!(report_f64(neon_marker_current, "draw_pipeline_binds"), 1.0);
   assert_eq!(report_f64(neon_marker_current, "draw_bind_group_binds"), 0.0);
   assert_eq!(report_f64(neon_marker_current, "draw_scissor_sets"), 1.0);
   assert_eq!(
       report_f64(neon_marker_current, "gpu_timestamp_passes"),
       report_f64(neon_marker_current, "render_passes")
   );
   assert_eq!(report_f64(direct_surface_current, "expected_image_draws"), 384.0);
    assert_eq!(report_f64(direct_surface_current, "expected_draw_items"), 385.0);
    assert_eq!(
        report_f64(direct_surface_current, "draw_items"),
        report_f64(direct_surface_current, "expected_draw_items")
    );
    assert_eq!(
        report_f64(direct_surface_current, "image_draws"),
        report_f64(direct_surface_current, "expected_image_draws")
    );
    assert_eq!(report_f64(direct_surface_current, "draw_passes"), 1.0);
    assert_eq!(report_f64(direct_surface_current, "clear_passes"), 0.0);
    assert_eq!(report_f64(direct_surface_current, "present_passes"), 0.0);
    assert_eq!(report_f64(direct_surface_current, "render_passes"), 1.0);
    assert_eq!(report_f64(direct_surface_current, "texture_copies"), 0.0);
    assert!(report_f64(direct_surface_current, "gpu_timestamp_total_ns") > 0.0);
    assert_eq!(
        report_f64(direct_surface_current, "gpu_timestamp_passes"),
        report_f64(direct_surface_current, "render_passes")
    );

    let summary = report_section_slice(report, "id_mask_summary");
    assert!(summary.contains("\"id\": \"web.wasm.webgpu.id_mask_compositor.current\""));
    assert!(report_f64(summary, "current_render_passes") > 0.0);
    assert!(report_f64(summary, "current_buffer_upload_bytes") > 0.0);
    assert_eq!(report_u64(summary, "vertices"), 9600);
    let direct_surface_summary = report_section_slice(report, "direct_surface_summary");
    assert!(direct_surface_summary
        .contains("\"id\": \"web.wasm.webgpu.direct_surface.current\""));
    assert!(!direct_surface_summary.contains("legacy_"));
    assert_eq!(
        report_f64(direct_surface_summary, "current_p50_ms"),
        report_f64(direct_surface_current, "p50_ms")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "current_draw_items"),
        report_f64(direct_surface_current, "draw_items")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "current_image_draws"),
        report_f64(direct_surface_current, "image_draws")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "current_render_passes"),
        report_f64(direct_surface_current, "render_passes")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "current_draw_passes"),
        report_f64(direct_surface_current, "draw_passes")
    );
    assert_eq!(report_f64(direct_surface_summary, "current_clear_passes"), 0.0);
    assert_eq!(report_f64(direct_surface_summary, "current_present_passes"), 0.0);
    assert_eq!(
        report_f64(direct_surface_summary, "current_texture_copies"),
        report_f64(direct_surface_current, "texture_copies")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "current_gpu_timestamp_total_ns"),
        report_f64(direct_surface_current, "gpu_timestamp_total_ns")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "current_gpu_timestamp_passes"),
        report_f64(direct_surface_current, "gpu_timestamp_passes")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "expected_draw_items"),
        report_f64(direct_surface_current, "expected_draw_items")
    );
    assert_eq!(
        report_f64(direct_surface_summary, "expected_image_draws"),
        report_f64(direct_surface_current, "expected_image_draws")
    );
    let browser_trace = report_section_slice(report, "browser_trace");
    assert!(browser_trace.contains("\"status\": \"collected\""));
    assert!(browser_trace.contains("\"capture_phase\": \"benchmark-report\""));
    assert!(browser_trace.contains("\"timing_source\": \"untraced-baseline-report\""));
    assert!(report_f64(browser_trace, "events") > 0.0);
    assert!(report_f64(browser_trace, "gpu_related_events") > 0.0);
    assert!(report_f64(browser_trace, "duration_us") > 0.0);
    assert!(report_f64(browser_trace, "category_count") > 0.0);
    assert_eq!(report_u64(browser_trace, "benchmark_trace_interval_count"), 13);
    assert!(browser_trace.contains("\"benchmark_trace_interval_labels\""));
    assert!(browser_trace.contains("\"benchmark_trace_intervals\""));
    assert!(report_f64(browser_trace, "webgpu_related_events") > 0.0);
    let browser_startup = report_section_slice(report, "browser_startup");
    assert!(browser_startup.contains("\"id\": \"web.wasm.webgpu.browser_startup\""));
    assert!(browser_startup.contains("\"source\": \"performance.now+node.fs.stat\""));
    assert!(report_f64(browser_startup, "wasm_init_ms") > 0.0);
    assert!(report_f64(browser_startup, "app_init_ms") > 0.0);
    assert!(report_f64(browser_startup, "report_ready_ms") > 0.0);
    assert!(report_f64(browser_startup, "wasm_memory_bytes") > 0.0);
    assert_eq!(report_u64(browser_startup, "package_file_count"), 4);
    assert!(report_f64(browser_startup, "package_bytes") > 0.0);
    assert!(report_f64(browser_startup, "wasm_bytes") > 0.0);
    assert!(report_f64(browser_startup, "js_bytes") > 0.0);
    let gpu_timestamp_stage_breakdown =
        report_section_slice(report, "gpu_timestamp_stage_breakdown");
    assert!(gpu_timestamp_stage_breakdown
        .contains("\"id\": \"web.wasm.webgpu.gpu_timestamp_stage_breakdown\""));
    assert_eq!(report_u64(gpu_timestamp_stage_breakdown, "row_count"), 17);
    assert_eq!(report_u64(gpu_timestamp_stage_breakdown, "collected_rows"), 17);
    assert_eq!(report_u64(gpu_timestamp_stage_breakdown, "stage_count"), 9);
    assert_eq!(report_u64(gpu_timestamp_stage_breakdown, "row_detail_count"), 17);
    assert_eq!(report_u64(gpu_timestamp_stage_breakdown, "total_render_passes"), 98);
    assert_eq!(
        report_u64(gpu_timestamp_stage_breakdown, "total_render_passes"),
        report_u64(gpu_timestamp_stage_breakdown, "total_timestamp_passes"),
    );
    assert_eq!(
        report_u64(gpu_timestamp_stage_breakdown, "total_render_passes"),
        report_u64(gpu_timestamp_stage_breakdown, "total_family_passes"),
    );
    assert_eq!(
        report_u64(gpu_timestamp_stage_breakdown, "total_timestamp_ns"),
        report_u64(gpu_timestamp_stage_breakdown, "total_family_timestamp_ns"),
    );
    assert!(gpu_timestamp_stage_breakdown.contains("\"stage\": \"draw\""));
    assert!(gpu_timestamp_stage_breakdown.contains("\"stage\": \"id_mask_field_jump\""));
    assert!(gpu_timestamp_stage_breakdown.contains("\"stage\": \"present\""));
    assert!(gpu_timestamp_stage_breakdown.contains("\"id\": \"web.wasm.webgpu.frame_loop\""));
    let warm_resource_churn = report_section_slice(report, "warm_resource_churn");
    assert!(warm_resource_churn
        .contains("\"id\": \"web.wasm.webgpu.warm_resource_churn.current_rows\""));
    assert_eq!(report_u64(warm_resource_churn, "checked_rows"), 15);
    assert_eq!(report_u64(warm_resource_churn, "excluded_rows"), 2);
    assert_eq!(report_u64(warm_resource_churn, "row_detail_count"), 15);
    assert!(warm_resource_churn.contains("\"row_details\": ["));
    assert_eq!(report_u64(warm_resource_churn, "total_buffer_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_texture_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_pipeline_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_sampler_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_mesh3d_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_draw_buffer_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_image_texture_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_image_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_target_texture_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_target_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_layer_texture_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_layer_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_scene3d_buffer_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_scene3d_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_effect_buffer_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_effect_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_id_mask_texture_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_id_mask_buffer_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_id_mask_bind_group_creates"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_image_upload_temp_allocs"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_image_upload_temp_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_image_upload_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_draw_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_draw_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_scene3d_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_scene3d_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_effect_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_effect_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_id_mask_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_id_mask_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_image_upload_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_image_upload_scratch_growth_bytes"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_resource_table_scratch_grows"), 0);
    assert_eq!(report_u64(warm_resource_churn, "total_cpu_resource_table_scratch_growth_bytes"), 0);

   let wasm_allocation_audit = report_section_slice(report, "wasm_allocation_audit");
   assert!(wasm_allocation_audit
       .contains("\"id\": \"web.wasm.webgpu.wasm_allocation_audit.current_rows\""));
   assert_eq!(report_u64(wasm_allocation_audit, "checked_count"), 15);
   assert_eq!(report_u64(wasm_allocation_audit, "excluded_count"), 2);
   assert_eq!(report_u64(wasm_allocation_audit, "row_detail_count"), 15);
   assert!(report_u64(wasm_allocation_audit, "total_wasm_alloc_count") > 0);
   assert!(report_u64(wasm_allocation_audit, "total_wasm_alloc_bytes") > 0);
   assert_eq!(report_u64(wasm_allocation_audit, "total_wasm_realloc_count"), 0);
    assert_eq!(report_u64(wasm_allocation_audit, "total_wasm_realloc_grow_bytes"), 0);
    assert!(report_f64(wasm_allocation_audit, "budget_wasm_allocs_per_frame") <= 7.0);
    assert!(report_f64(wasm_allocation_audit, "budget_wasm_alloc_bytes_per_frame") <= 144.0);
    assert!(
        report_f64(wasm_allocation_audit, "max_wasm_allocs_per_frame")
            <= report_f64(wasm_allocation_audit, "budget_wasm_allocs_per_frame")
    );
    assert!(
        report_f64(wasm_allocation_audit, "max_wasm_alloc_bytes_per_frame")
            <= report_f64(wasm_allocation_audit, "budget_wasm_alloc_bytes_per_frame")
    );

    let wasm_allocation_invariance = report_section_slice(report, "wasm_allocation_invariance");
    assert!(wasm_allocation_invariance
        .contains("\"id\": \"web.wasm.webgpu.wasm_allocation_invariance.current_rows\""));
    assert!(wasm_allocation_invariance.contains("\"status\": \"shared-submit-boundary-profile\""));
    assert!(
        wasm_allocation_invariance.contains("\"reference_row\": \"web.wasm.webgpu.frame_loop\"")
    );
    assert_eq!(
        report_u64(wasm_allocation_invariance, "checked_count"),
        report_u64(wasm_allocation_audit, "checked_count"),
    );
    assert_eq!(report_u64(wasm_allocation_invariance, "unique_signature_count"), 1);
    assert_eq!(
        report_u64(wasm_allocation_invariance, "shared_wasm_alloc_count"),
        report_u64(frame, "wasm_alloc_count"),
    );
    assert_eq!(
        report_u64(wasm_allocation_invariance, "shared_wasm_alloc_bytes"),
        report_u64(frame, "wasm_alloc_bytes"),
    );
    assert_eq!(report_u64(wasm_allocation_invariance, "shared_wasm_realloc_count"), 0);
    assert_eq!(report_u64(wasm_allocation_invariance, "shared_wasm_realloc_grow_bytes"), 0,);
    let frame_stage_allocations = report_section_slice(report, "frame_loop_wasm_allocation_stages");
    assert!(frame_stage_allocations
        .contains("\"id\": \"web.wasm.webgpu.frame_loop_wasm_allocation_stages\""));
    assert!(frame_stage_allocations.contains("\"row_id\": \"web.wasm.webgpu.frame_loop\""));
    assert_eq!(report_u64(frame_stage_allocations, "stage_count"), 11);
    assert_eq!(
        report_u64(frame_stage_allocations, "total_stage_wasm_alloc_count"),
        report_u64(frame, "wasm_alloc_count"),
    );
    assert_eq!(
        report_u64(frame_stage_allocations, "total_stage_wasm_alloc_bytes"),
        report_u64(frame, "wasm_alloc_bytes"),
    );
    assert_eq!(report_u64(frame_stage_allocations, "total_stage_wasm_realloc_count"), 0);
    assert_eq!(report_u64(frame_stage_allocations, "total_stage_wasm_realloc_grow_bytes"), 0,);
    assert!(frame_stage_allocations.contains("\"stage\": \"router_draw\""));
    assert!(frame_stage_allocations.contains("\"stage\": \"encode_pass\""));
    let submit_stage_allocations =
        report_section_slice(report, "frame_loop_wasm_submit_allocation_stages");
    assert!(submit_stage_allocations
        .contains("\"id\": \"web.wasm.webgpu.frame_loop_wasm_submit_allocation_stages\""));
    assert!(submit_stage_allocations.contains("\"row_id\": \"web.wasm.webgpu.frame_loop\""));
    assert_eq!(report_u64(submit_stage_allocations, "stage_count"), 9);
    assert_eq!(
        report_u64(submit_stage_allocations, "total_stage_wasm_alloc_count"),
        report_u64(frame, "submit_total_alloc_count"),
    );
    assert_eq!(
        report_u64(submit_stage_allocations, "total_stage_wasm_alloc_bytes"),
        report_u64(frame, "submit_total_alloc_bytes"),
    );
    assert_eq!(
        report_u64(submit_stage_allocations, "frame_stage_submit_wasm_alloc_count"),
        report_u64(frame, "wasm_stage_submit_alloc_count"),
    );
    assert_eq!(
        report_u64(frame, "submit_total_alloc_count"),
        report_u64(frame, "wasm_stage_submit_alloc_count"),
    );
    assert_eq!(
        report_u64(frame, "submit_total_alloc_bytes"),
        report_u64(frame, "wasm_stage_submit_alloc_bytes"),
    );
    assert_eq!(report_u64(frame, "submit_total_realloc_count"), 0);
    assert_eq!(report_u64(frame, "submit_total_realloc_grow_bytes"), 0);
    assert_eq!(report_u64(frame, "submit_upload_alloc_count"), 0);
    assert_eq!(report_u64(frame, "submit_encoder_alloc_count"), 0);
    assert_eq!(report_u64(frame, "submit_render_alloc_count"), 0);
    assert_eq!(report_u64(frame, "submit_timestamp_alloc_count"), 0);
    assert_eq!(report_u64(frame, "submit_scratch_stats_alloc_count"), 0);
    assert_eq!(report_u64(frame, "submit_present_alloc_count"), 0);
    assert!(report_u64(frame, "submit_surface_alloc_count") > 0);
    assert!(report_u64(frame, "submit_finish_queue_alloc_count") > 0);
    assert!(report_u64(frame, "submit_timestamp_map_alloc_count") > 0);
    assert!(submit_stage_allocations.contains("\"dominant_stage\": \"surface\""));
    assert!(submit_stage_allocations.contains("\"stage\": \"finish_queue\""));
    let backend_path_coverage = report_section_slice(report, "backend_path_coverage");
    assert!(backend_path_coverage.contains("\"id\": \"web.wasm.webgpu.backend_path_coverage\""));
    assert_eq!(report_u64(backend_path_coverage, "expected_path_count"), 15);
    assert_eq!(report_u64(backend_path_coverage, "covered_path_count"), 15);
    assert_eq!(report_u64(backend_path_coverage, "missing_path_count"), 0);
    assert!(backend_path_coverage.contains("\"id\": \"glyph_atlas_upload\""));
    assert!(backend_path_coverage.contains("\"id\": \"image_upload\""));
    assert!(backend_path_coverage.contains("\"id\": \"backdrop_batch\""));
    assert!(backend_path_coverage
        .contains("\"web.wasm.webgpu.backdrop_batch.current_coalesced\""));
    assert!(!backend_path_coverage
        .contains("\"web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy\""));
    assert!(backend_path_coverage.contains("\"id\": \"mixed_text_image_effects\""));
    assert!(!backend_path_coverage
        .contains("\"web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched\""));
    assert!(backend_path_coverage.contains("\"id\": \"layer_damage_effects\""));
    assert!(!backend_path_coverage
        .contains("\"web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched\""));
    assert!(backend_path_coverage.contains("\"id\": \"clean_layer_reuse\""));
    assert!(backend_path_coverage.contains("\"web.wasm.webgpu.clean_layer.clean_reuse\""));
    assert!(!backend_path_coverage.contains("\"web.wasm.webgpu.clean_layer.dirty_rerender\""));
    assert!(backend_path_coverage.contains("\"id\": \"command_family_matrix\""));
    assert!(backend_path_coverage.contains("\"web.wasm.webgpu.command_family_matrix\""));
    assert!(
        !backend_path_coverage.contains("\"web.wasm.webgpu.command_family_matrix.legacy_rebind\"")
    );
    assert!(backend_path_coverage.contains("\"id\": \"glyph_run\""));
    assert!(backend_path_coverage.contains("\"web.wasm.webgpu.glyph_run.current\""));
    assert!(!backend_path_coverage.contains("\"web.wasm.webgpu.glyph_run.legacy_rebind\""));
    assert!(backend_path_coverage.contains("\"id\": \"neon_marker\""));
    assert!(backend_path_coverage.contains("\"web.wasm.webgpu.neon_marker.current\""));
    assert!(!backend_path_coverage.contains("\"web.wasm.webgpu.neon_marker.legacy_rebind\""));
    assert!(backend_path_coverage.contains("\"id\": \"direct_surface\""));
    assert!(backend_path_coverage.contains("\"web.wasm.webgpu.direct_surface.current\""));
    assert!(
        !backend_path_coverage.contains("\"web.wasm.webgpu.direct_surface.legacy_scene_present\"")
    );
    let upload_summary = report_section_slice(report, "upload_summary");
    assert!(upload_summary.contains("\"id\": \"web.wasm.webgpu.upload.current_dirty\""));
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.glyph_atlas_upload.legacy_full\""));
    assert!(!report.contains("\"id\": \"web.wasm.webgpu.image_upload.legacy_full\""));
    assert_eq!(
        report_f64(upload_summary, "glyph_current_texture_upload_bytes"),
        report_f64(glyph_current, "texture_upload_bytes")
    );
    assert_eq!(
        report_f64(upload_summary, "image_current_texture_upload_bytes"),
        report_f64(image_current, "texture_upload_bytes")
    );
    assert_eq!(
        report_f64(upload_summary, "glyph_current_gpu_timestamp_total_ns"),
        report_f64(glyph_current, "gpu_timestamp_total_ns")
    );
    assert_eq!(
        report_f64(upload_summary, "image_current_gpu_timestamp_total_ns"),
        report_f64(image_current, "gpu_timestamp_total_ns")
    );
    assert_eq!(
        report_f64(upload_summary, "atlas_dirty_width"),
        report_f64(glyph_current, "dirty_width")
    );
    assert_eq!(
        report_f64(upload_summary, "image_dirty_width"),
        report_f64(image_current, "dirty_width")
    );
    let effect_uniform_summary = report_section_slice(report, "effect_uniform_summary");
    assert!(effect_uniform_summary
        .contains("\"id\": \"web.wasm.webgpu.effect_uniform.current_batched\""));
    assert_eq!(report_u64(effect_uniform_summary, "current_effect_uniform_writes"), 1);
    assert!(report_u64(effect_uniform_summary, "current_effect_uniform_bytes") > 0);
    assert_eq!(
        report_f64(effect_uniform_summary, "current_effect_uniform_slots"),
        report_f64(effect_uniform_summary, "expected_backdrops")
    );
    assert_eq!(
        report_f64(effect_uniform_summary, "current_texture_copies"),
        report_f64(effect_current, "texture_copies")
    );
    assert_eq!(
        report_f64(effect_uniform_summary, "current_gpu_timestamp_passes"),
        report_f64(effect_current, "gpu_timestamp_passes")
    );
    assert_eq!(
        report_f64(effect_uniform_summary, "current_gpu_timestamp_total_ns"),
        report_f64(effect_current, "gpu_timestamp_total_ns")
    );
    let backdrop_batch_summary = report_section_slice(report, "backdrop_batch_summary");
    assert!(backdrop_batch_summary
        .contains("\"id\": \"web.wasm.webgpu.backdrop_batch.current\""));
    assert_eq!(
        report_f64(backdrop_batch_summary, "current_effect_uniform_writes"),
        report_f64(backdrop_batch_current, "effect_uniform_writes")
    );
    assert_eq!(
        report_f64(backdrop_batch_summary, "current_effect_uniform_slots"),
        report_f64(backdrop_batch_current, "effect_uniform_slots")
    );
    assert_eq!(
        report_f64(backdrop_batch_summary, "current_texture_copies"),
        report_f64(backdrop_batch_current, "texture_copies")
    );
    assert_eq!(
        report_f64(backdrop_batch_summary, "current_render_passes"),
        report_f64(backdrop_batch_current, "render_passes")
    );
    let scene3d_summary = report_section_slice(report, "scene3d_summary");
    assert!(scene3d_summary
        .contains("\"id\": \"web.wasm.webgpu.scene3d.reused_mesh_vs_recreate_mesh\""));
    assert!(report_f64(scene3d_summary, "recreate_over_reused") >= 0.0);
    assert_eq!(report_u64(scene3d_summary, "reused_mesh3d_creates"), 0);
    assert!(report_u64(scene3d_summary, "recreate_mesh3d_creates") > 0);
    assert_eq!(report_u64(scene3d_summary, "reused_buffer_grows"), 0);
    assert!(report_u64(scene3d_summary, "recreate_buffer_grows") > 0);
    assert_eq!(report_u64(scene3d_summary, "reused_cpu_scratch_grows"), 0);
    assert_eq!(report_u64(scene3d_summary, "reused_cpu_scratch_growth_bytes"), 0);
    let scene3d_stress_summary = report_section_slice(report, "scene3d_stress_summary");
    assert!(scene3d_stress_summary.contains(
        "\"id\": \"web.wasm.webgpu.scene3d.stress_reused_mesh_vs_stress_recreate_mesh\""
    ));
    assert!(report_f64(scene3d_stress_summary, "recreate_over_reused") >= 0.0);
    assert_eq!(report_u64(scene3d_stress_summary, "reused_mesh3d_creates"), 0);
    assert!(report_u64(scene3d_stress_summary, "recreate_mesh3d_creates") > 0);
    assert_eq!(report_u64(scene3d_stress_summary, "reused_buffer_grows"), 0);
    assert!(report_u64(scene3d_stress_summary, "recreate_buffer_grows") > 0);
    assert_eq!(report_u64(scene3d_stress_summary, "reused_cpu_scratch_grows"), 0);
    assert_eq!(report_u64(scene3d_stress_summary, "reused_cpu_scratch_growth_bytes"), 0);
    assert!(report_u64(scene3d_stress_summary, "instances") >= 64);
    let mixed_summary = report_section_slice(report, "mixed_summary");
    assert!(mixed_summary.contains(
        "\"id\": \"web.wasm.webgpu.mixed_text_image_effects.current\""
    ));
    assert_eq!(report_f64(mixed_summary, "current_p50_ms"), report_f64(mixed, "p50_ms"));
    assert_eq!(
        report_f64(mixed_summary, "current_draw_pipeline_binds"),
        report_f64(mixed, "draw_pipeline_binds")
    );
    assert_eq!(
        report_f64(mixed_summary, "current_draw_bind_group_binds"),
        report_f64(mixed, "draw_bind_group_binds")
    );
    assert_eq!(
        report_f64(mixed_summary, "current_draw_scissor_sets"),
        report_f64(mixed, "draw_scissor_sets")
    );
    assert_eq!(
        report_f64(mixed_summary, "current_effect_uniform_writes"),
        report_f64(mixed, "effect_uniform_writes")
    );
    let layer_effects_summary = report_section_slice(report, "layer_effects_summary");
    assert!(layer_effects_summary.contains(
        "\"id\": \"web.wasm.webgpu.layer_damage_effects.current\""
    ));
    assert_eq!(
        report_f64(layer_effects_summary, "current_p50_ms"),
        report_f64(layer_effects, "p50_ms")
    );
    assert_eq!(
        report_f64(layer_effects_summary, "current_draw_pipeline_binds"),
        report_f64(layer_effects, "draw_pipeline_binds")
    );
    assert_eq!(
        report_f64(layer_effects_summary, "current_draw_bind_group_binds"),
        report_f64(layer_effects, "draw_bind_group_binds")
    );
    assert_eq!(
        report_f64(layer_effects_summary, "current_draw_scissor_sets"),
        report_f64(layer_effects, "draw_scissor_sets")
    );
    assert_eq!(
        report_f64(layer_effects_summary, "current_effect_uniform_writes"),
        report_f64(layer_effects, "effect_uniform_writes")
    );
    assert_eq!(
        report_f64(layer_effects_summary, "current_texture_copies"),
        report_f64(layer_effects, "texture_copies")
    );
    assert_eq!(
        report_f64(layer_effects_summary, "current_render_passes"),
        report_f64(layer_effects, "render_passes")
    );
    let clean_layer_summary = report_section_slice(report, "clean_layer_summary");
    assert!(clean_layer_summary
        .contains("\"id\": \"web.wasm.webgpu.clean_layer.clean_reuse\""));
    assert!(!clean_layer_summary.contains("dirty_"));
    assert_eq!(report_f64(clean_layer_summary, "clean_p50_ms"), report_f64(clean_layer, "p50_ms"));
    assert_eq!(
        report_f64(clean_layer_summary, "clean_draw_items"),
        report_f64(clean_layer, "draw_items")
    );
    assert_eq!(report_f64(clean_layer_summary, "clean_layer_cache_hits"), 1.0);
    assert_eq!(report_f64(clean_layer_summary, "clean_layer_cache_misses"), 0.0);
    assert_eq!(report_f64(clean_layer_summary, "clean_layer_passes"), 0.0);
    assert!(
        report_f64(clean_layer_summary, "clean_layer_cache_skipped_draws")
            > report_f64(clean_layer_summary, "clean_draw_items")
    );
    assert!(
        report_f64(clean_layer_summary, "clean_gpu_timestamp_total_ns")
            == report_f64(clean_layer, "gpu_timestamp_total_ns")
    );
    let command_family_summary = report_section_slice(report, "command_family_summary");
    assert!(command_family_summary
        .contains("\"id\": \"web.wasm.webgpu.command_family_matrix.current\""));
    assert_eq!(
        report_f64(command_family_summary, "current_p50_ms"),
        report_f64(command_family, "p50_ms")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_draw_items"),
        report_f64(command_family, "draw_items")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_draw_pipeline_binds"),
        report_f64(command_family, "draw_pipeline_binds")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_draw_bind_group_binds"),
        report_f64(command_family, "draw_bind_group_binds")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_draw_scissor_sets"),
        report_f64(command_family, "draw_scissor_sets")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_image_mesh_draws"),
        report_f64(command_family, "image_mesh_draws")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_nine_slice_draws"),
        report_f64(command_family, "nine_slice_draws")
    );
    assert_eq!(
        report_f64(command_family_summary, "current_sdf_glyph_quads"),
        report_f64(command_family, "sdf_glyph_quads")
    );
    assert_eq!(report_f64(command_family_summary, "current_camera_bg_draws"), 0.0);
    assert_eq!(report_f64(command_family_summary, "expected_camera_bg"), 0.0);
    let glyph_run_summary = report_section_slice(report, "glyph_run_summary");
    assert!(glyph_run_summary.contains("\"id\": \"web.wasm.webgpu.glyph_run.current\""));
    assert_eq!(
        report_f64(glyph_run_summary, "current_p50_ms"),
        report_f64(glyph_run_current, "p50_ms")
    );
    assert_eq!(report_u64(glyph_run_summary, "expected_glyph_runs"), 64);
    assert_eq!(report_u64(glyph_run_summary, "expected_glyphs_per_run"), 8);
    assert_eq!(report_u64(glyph_run_summary, "expected_glyph_quads"), 512);
    assert_eq!(report_u64(glyph_run_summary, "expected_sdf_runs"), 32);
    assert_eq!(report_u64(glyph_run_summary, "expected_sdf_glyph_quads"), 256);
    assert_eq!(
        report_f64(glyph_run_summary, "current_draw_items"),
        report_f64(glyph_run_current, "draw_items")
    );
    assert_eq!(
        report_f64(glyph_run_summary, "current_glyph_quads"),
        report_f64(glyph_run_current, "glyph_quads")
    );
    assert_eq!(
        report_f64(glyph_run_summary, "current_sdf_glyph_quads"),
        report_f64(glyph_run_current, "sdf_glyph_quads")
    );
    assert!(report_f64(glyph_run_summary, "current_draw_pipeline_binds") > 0.0);
    assert!(report_f64(glyph_run_summary, "current_draw_bind_group_binds") > 0.0);
    assert!(report_f64(glyph_run_summary, "current_draw_scissor_sets") > 0.0);
    let neon_marker_summary = report_section_slice(report, "neon_marker_summary");
    assert!(neon_marker_summary
        .contains("\"id\": \"web.wasm.webgpu.neon_marker.current\""));
    assert!(!neon_marker_summary.contains("legacy_"));
    assert_eq!(
        report_f64(neon_marker_summary, "current_p50_ms"),
        report_f64(neon_marker_current, "p50_ms")
    );
    assert_eq!(
        report_f64(neon_marker_summary, "current_draw_items"),
        report_f64(neon_marker_current, "draw_items")
    );
    assert_eq!(
        report_f64(neon_marker_summary, "current_solid_tris"),
        report_f64(neon_marker_current, "solid_tris")
    );
    assert_eq!(report_u64(neon_marker_summary, "expected_markers"), 64);
    assert_eq!(report_u64(neon_marker_summary, "expected_draw_items"), 192);
    assert_eq!(
        report_f64(neon_marker_summary, "current_draw_bind_group_binds"),
        report_f64(neon_marker_current, "draw_bind_group_binds")
    );
    assert_eq!(
        report_f64(neon_marker_summary, "current_draw_pipeline_binds"),
        report_f64(neon_marker_current, "draw_pipeline_binds")
    );
    assert_eq!(
        report_f64(neon_marker_summary, "current_draw_scissor_sets"),
        report_f64(neon_marker_current, "draw_scissor_sets")
    );
    let pixel_check = report_section_slice(report, "pixel_check");
    assert!(pixel_check.contains("\"target\": \"app\""));
    assert_eq!(report_f64(pixel_check, "mse"), 0.0);
    assert_eq!(report_u64(pixel_check, "pixdiff"), 0);
    assert_eq!(report_u64(pixel_check, "max_err"), 0);
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
