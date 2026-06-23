#[test]
fn renderer_loads_build_time_metallib_instead_of_runtime_source() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("const DEFAULT_METALLIB")
            && source.contains("include_bytes!(concat!(env!(\"OUT_DIR\"), \"/default.metallib\"))")
            && source.contains("new_library_with_data(DEFAULT_METALLIB)")
            && !source.contains("new_library_with_source"),
        "renderer-metal must load the build-time metallib and avoid runtime shader source compilation"
    );
}

#[test]
fn build_script_fails_apple_metallib_generation_instead_of_placeholder_fallback() {
    let source = include_str!("../build.rs");
    assert!(
        source.contains("target_is_apple")
            && source.contains("Metal toolchain not found")
            && source.contains("metal compile failed")
            && source.contains("metallib link failed")
            && !source.contains("Metal toolchain not found; emitting placeholder metallib")
            && !source.contains("metallib link failed; emitting placeholder metallib"),
        "renderer-metal build.rs must not emit placeholder metallibs for Apple renderer builds"
    );
}

#[test]
fn per_frame_reuse_never_waits_for_gpu_completion() {
    let source = include_str!("../src/lib.rs");
    let start = source.find("fn prepare_for_encode").expect("prepare_for_encode function");
    let tail = &source[start..];
    let end = tail.find("fn mark_submitted").expect("mark_submitted function");
    let prepare_for_encode = &tail[..end];
    assert!(
        !prepare_for_encode.contains("wait_until_completed"),
        "normal frame-ring reuse must not block the CPU on an in-flight Metal command buffer"
    );
    assert!(
        source.contains("frame_backpressure_skipped")
            && source.contains(".find(|slot| self.frames[*slot].is_available())"),
        "renderer-metal must select an available frame-ring slot or skip instead of blocking"
    );
}

#[test]
fn blocking_gpu_waits_are_limited_to_explicit_readback_helpers() {
    let source = include_str!("../src/lib.rs");
    let total_waits = source.matches("wait_until_completed").count();
    let readback_texture =
        source_block(source, "fn readback_texture_bgra8", "fn readback_direct_live_camera_bgra8");
    let readback_camera =
        source_block(source, "fn readback_direct_live_camera_bgra8", "pub fn readback_bgra8");
    let allowed_waits = readback_texture.matches("wait_until_completed").count()
        + readback_camera.matches("wait_until_completed").count();

    assert_eq!(
        total_waits, allowed_waits,
        "renderer-metal must keep blocking GPU waits out of frame hot paths"
    );
    assert_eq!(allowed_waits, 2, "readback helpers are the only allowed blocking waits");
}

#[test]
fn command_buffer_gpu_duration_is_enabled_on_macos_and_ios() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("#[cfg(any(target_os = \"ios\", target_os = \"macos\"))]")
            && source.contains("GPUStartTime")
            && source.contains("GPUEndTime"),
        "direct command-buffer GPU duration must be compiled for both iOS device reports and macOS Metal A/B perf runs"
    );
}

#[test]
fn completed_gpu_duration_is_attributed_to_frame_id() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("struct CompletedGpuStats")
            && source.contains("gpu_frame_id")
            && source.contains("frame_id,")
            && source.contains("stats.gpu_frame_id = gpu.frame_id"),
        "published GPU durations must carry the completed frame id so perf reports do not sample stale command-buffer timings"
    );
}

#[test]
fn metal_draw_cmd_debug_capture_names_are_frozen() {
    let source = include_str!("../src/lib.rs");
    let mapping = source_without_whitespace(source_block(
        source,
        "fn draw_cmd_kind",
        "#[inline(always)]\nfn running_on_ios_simulator",
    ));
    let expected = [
        r#"api::DrawCmd::LayerBegin{..}=>"layer_begin""#,
        r#"api::DrawCmd::LayerEnd=>"layer_end""#,
        r#"api::DrawCmd::Solid{..}=>"solid""#,
        r#"api::DrawCmd::Image{..}=>"image""#,
        r#"api::DrawCmd::ImageMesh{..}=>"image_mesh""#,
        r#"api::DrawCmd::GlyphRun{..}=>"glyph_run""#,
        r#"api::DrawCmd::RRect{..}=>"rrect""#,
        r#"api::DrawCmd::NineSlice{..}=>"nine_slice""#,
        r#"api::DrawCmd::Backdrop{..}=>"backdrop""#,
        r#"api::DrawCmd::VisualEffect{..}=>"visual_effect""#,
        r#"api::DrawCmd::CameraBg{..}=>"camera_bg""#,
        r#"api::DrawCmd::Spinner{..}=>"spinner""#,
        r#"api::DrawCmd::ClipPush{..}=>"clip_push""#,
        r#"api::DrawCmd::ClipPop=>"clip_pop""#,
    ];
    let mut previous = 0usize;
    for pattern in expected {
        let offset = mapping[previous..]
            .find(pattern)
            .map(|relative| previous + relative)
            .unwrap_or_else(|| panic!("missing Metal draw command debug mapping {pattern}"));
        previous = offset + pattern.len();
    }
    assert_eq!(mapping.matches("api::DrawCmd::").count(), expected.len());
}

fn source_block<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_idx = source.find(start).expect("source block start");
    let tail = &source[start_idx..];
    let end_idx = tail.find(end).expect("source block end");
    &tail[..end_idx]
}

fn source_without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[cfg(target_os = "macos")]
#[test]
fn renderer_initializes_default_pipelines_from_embedded_metallib_on_macos() {
    use oxide_renderer_metal::{MetalInitError, MetalRenderer};

    match MetalRenderer::new_default() {
        Ok(_) => {}
        Err(MetalInitError::NoDevice) => {
            panic!("macOS Metal performance contract requires a real Metal device")
        }
        Err(err) => {
            panic!(
            "renderer must initialize from embedded default.metallib without runtime shader fallback: {err}"
         )
        }
    }
}
