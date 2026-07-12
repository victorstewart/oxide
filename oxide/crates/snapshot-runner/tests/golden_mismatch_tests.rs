use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

static RUNNER_BIN: OnceLock<PathBuf> = OnceLock::new();

fn runner_bin() -> &'static Path {
    RUNNER_BIN.get_or_init(|| {
        let current_exe = std::env::current_exe().expect("current integration test executable");
        let profile_dir =
            current_exe.parent().and_then(Path::parent).expect("target profile directory");
        let bin = profile_dir.join("oxide-snapshot-runner");
        if !bin.exists() {
            let jobs =
                thread::available_parallelism().map(|jobs| jobs.get()).unwrap_or(1).to_string();
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let status = Command::new(cargo)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .arg("build")
                .arg("--locked")
                .arg("-j")
                .arg(jobs)
                .arg("--bin")
                .arg("oxide-snapshot-runner")
                .status()
                .expect("build snapshot runner binary");
            assert!(status.success(), "failed to build snapshot runner binary");
        }
        bin
    })
}

fn temp_case_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    path.push(format!("oxide-snapshot-runner-{}-{}-{}", name, std::process::id(), nanos));
    fs::create_dir_all(&path).expect("create temp case directory");
    path
}

fn run_snapshot(
    component: &str,
    width: u32,
    height: u32,
    scale: f32,
    out: &Path,
    golden: &Path,
    allow_mismatch: bool,
) -> Output {
    let mut command = Command::new(runner_bin());
    command
        .arg("--suite")
        .arg("static")
        .arg("--component")
        .arg(component)
        .arg("--width")
        .arg(width.to_string())
        .arg("--height")
        .arg(height.to_string())
        .arg("--scale")
        .arg(format_snapshot_scale(scale))
        .arg("--out")
        .arg(out)
        .arg("--golden")
        .arg(golden);
    if allow_mismatch {
        command.arg("--allow-mismatch");
    }
    command.output().expect("run snapshot runner")
}

fn run_snapshot_checked(
    component: &str,
    width: u32,
    height: u32,
    scale: f32,
    out: &Path,
    golden: &Path,
    pixel_tolerance: usize,
    layer_cache: Option<&str>,
) -> Output {
    let mut command = Command::new(runner_bin());
    command
        .arg("--suite")
        .arg("static")
        .arg("--component")
        .arg(component)
        .arg("--width")
        .arg(width.to_string())
        .arg("--height")
        .arg(height.to_string())
        .arg("--scale")
        .arg(format_snapshot_scale(scale))
        .arg("--out")
        .arg(out)
        .arg("--golden")
        .arg(golden)
        .arg("--pixel-tolerance")
        .arg(pixel_tolerance.to_string())
        .arg("--max-error-tolerance")
        .arg("3")
        .arg("--mse-tolerance")
        .arg("0.02");
    if let Some(enabled) = layer_cache {
        command.env("OXIDE_ENABLE_LAYER_CACHE", enabled);
    }
    command.output().expect("run checked snapshot")
}

fn format_snapshot_scale(scale: f32) -> String {
    if (scale - scale.round()).abs() < f32::EPSILON {
        format!("{}", scale.round() as u32)
    } else {
        format!("{scale}")
    }
}

fn png_size(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).expect("read png");
    let decoder = png::Decoder::new(&bytes[..]);
    let reader = decoder.read_info().expect("read png info");
    let info = reader.info();
    (info.width, info.height)
}

fn png_signal_counts(path: &Path) -> (usize, usize) {
    let bytes = fs::read(path).expect("read png");
    let decoder = png::Decoder::new(&bytes[..]);
    let mut reader = decoder.read_info().expect("read png info");
    let mut out = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out).expect("read png frame");
    let bytes = &out[..info.buffer_size()];
    let mut bright = 0;
    let mut dark = 0;
    match info.color_type {
        png::ColorType::Rgba => {
            for pixel in bytes.chunks_exact(4) {
                if pixel[0] > 220 && pixel[1] > 220 && pixel[2] > 220 {
                    bright += 1;
                }
                if pixel[0] < 16 && pixel[1] < 16 && pixel[2] < 16 {
                    dark += 1;
                }
            }
        }
        png::ColorType::Rgb => {
            for pixel in bytes.chunks_exact(3) {
                if pixel[0] > 220 && pixel[1] > 220 && pixel[2] > 220 {
                    bright += 1;
                }
                if pixel[0] < 16 && pixel[1] < 16 && pixel[2] < 16 {
                    dark += 1;
                }
            }
        }
        _ => panic!("unsupported png color type for signal count"),
    }
    (bright, dark)
}

fn png_non_white_count(path: &Path) -> usize {
    let bytes = fs::read(path).expect("read png");
    let decoder = png::Decoder::new(&bytes[..]);
    let mut reader = decoder.read_info().expect("read png info");
    let mut out = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out).expect("read png frame");
    let bytes = &out[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => bytes
            .chunks_exact(4)
            .filter(|pixel| pixel[0] < 245 || pixel[1] < 245 || pixel[2] < 245)
            .count(),
        png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .filter(|pixel| pixel[0] < 245 || pixel[1] < 245 || pixel[2] < 245)
            .count(),
        _ => panic!("unsupported png color type for non-white count"),
    }
}

fn png_color_signal_count(path: &Path) -> usize {
    let bytes = fs::read(path).expect("read png");
    let decoder = png::Decoder::new(&bytes[..]);
    let mut reader = decoder.read_info().expect("read png info");
    let mut out = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out).expect("read png frame");
    let bytes = &out[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => bytes
            .chunks_exact(4)
            .filter(|pixel| {
                let hi = pixel[0].max(pixel[1]).max(pixel[2]);
                let lo = pixel[0].min(pixel[1]).min(pixel[2]);
                hi > 32 && hi.saturating_sub(lo) > 24
            })
            .count(),
        png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .filter(|pixel| {
                let hi = pixel[0].max(pixel[1]).max(pixel[2]);
                let lo = pixel[0].min(pixel[1]).min(pixel[2]);
                hi > 32 && hi.saturating_sub(lo) > 24
            })
            .count(),
        _ => panic!("unsupported png color type for color signal count"),
    }
}

fn create_button_golden(dir: &Path) -> PathBuf {
    let golden = dir.join("golden.png");
    let out = dir.join("button.png");
    let output = run_snapshot("button", 160, 120, 1.0, &out, &golden, false);
    assert!(
        output.status.success(),
        "initial golden creation failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    golden
}

#[test]
fn existing_golden_pixel_mismatch_fails_by_default() {
    let dir = temp_case_dir("pixel-default");
    let golden = create_button_golden(&dir);
    let out = dir.join("spinner.png");

    let output = run_snapshot("spinner", 160, 120, 1.0, &out, &golden, false);

    assert!(!output.status.success(), "pixel mismatch unexpectedly passed");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("golden mismatch"),
        "expected golden mismatch stderr, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn existing_golden_size_mismatch_fails_by_default() {
    let dir = temp_case_dir("size-default");
    let golden = create_button_golden(&dir);
    let out = dir.join("button-resized.png");

    let output = run_snapshot("button", 168, 120, 1.0, &out, &golden, false);

    assert!(!output.status.success(), "size mismatch unexpectedly passed");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("golden size mismatch"),
        "expected golden size mismatch stderr, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn committed_renderer_goldens_cover_scene3d_damage_camera_and_id_mask() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let golden_dir = manifest.join("../..").join("goldens/snapshots");
    let cases = [
        ("scene3d_mixed", "scene3d_mixed", 192_u32, 192_u32, 1.0_f32),
        ("scene3d_bloom", "scene3d_bloom", 192, 192, 1.0),
        ("scene3d_depth_stack", "scene3d_depth_stack", 192, 192, 1.0),
        ("scene3d_viewport_clip", "scene3d_viewport_clip", 192, 192, 1.0),
        ("scene3d_material_cull", "scene3d_material_cull", 192, 192, 1.0),
        ("scene3d_blend_modes", "scene3d_blend_modes", 192, 192, 1.0),
        ("scene3d_mixed", "scene3d_mixed_scale2", 384, 384, 2.0),
        ("scene3d_bloom", "scene3d_bloom_scale2", 384, 384, 2.0),
        ("scene3d_depth_stack", "scene3d_depth_stack_scale2", 384, 384, 2.0),
        ("scene3d_viewport_clip", "scene3d_viewport_clip_scale2", 384, 384, 2.0),
        ("scene3d_material_cull", "scene3d_material_cull_scale2", 384, 384, 2.0),
        ("scene3d_blend_modes", "scene3d_blend_modes_scale2", 384, 384, 2.0),
        ("scene3d_mixed", "scene3d_mixed_scale3", 576, 576, 3.0),
        ("scene3d_bloom", "scene3d_bloom_scale3", 576, 576, 3.0),
        ("scene3d_depth_stack", "scene3d_depth_stack_scale3", 576, 576, 3.0),
        ("scene3d_viewport_clip", "scene3d_viewport_clip_scale3", 576, 576, 3.0),
        ("scene3d_material_cull", "scene3d_material_cull_scale3", 576, 576, 3.0),
        ("scene3d_blend_modes", "scene3d_blend_modes_scale3", 576, 576, 3.0),
        ("scene3d_mixed", "scene3d_mixed_wide", 320, 192, 1.0),
        ("scene3d_bloom", "scene3d_bloom_wide", 320, 192, 1.0),
        ("scene3d_depth_stack", "scene3d_depth_stack_wide", 320, 192, 1.0),
        ("scene3d_viewport_clip", "scene3d_viewport_clip_wide", 320, 192, 1.0),
        ("scene3d_material_cull", "scene3d_material_cull_wide", 320, 192, 1.0),
        ("scene3d_blend_modes", "scene3d_blend_modes_wide", 320, 192, 1.0),
        ("scene3d_mixed", "scene3d_mixed_portrait", 192, 320, 1.0),
        ("scene3d_bloom", "scene3d_bloom_portrait", 192, 320, 1.0),
        ("scene3d_depth_stack", "scene3d_depth_stack_portrait", 192, 320, 1.0),
        ("scene3d_viewport_clip", "scene3d_viewport_clip_portrait", 192, 320, 1.0),
        ("scene3d_material_cull", "scene3d_material_cull_portrait", 192, 320, 1.0),
        ("scene3d_blend_modes", "scene3d_blend_modes_portrait", 192, 320, 1.0),
        ("scene_damage", "scene_damage", 192, 192, 1.0),
        ("scene_anim_timeline", "scene_anim_timeline", 192, 192, 1.0),
        ("scene_input_lab", "scene_input_lab", 192, 192, 1.0),
        ("scene_nine_slice", "scene_nine_slice", 192, 192, 1.0),
        ("scene_sdf_text", "scene_sdf_text", 256, 256, 1.0),
        ("scene_snapshot", "scene_snapshot", 256, 256, 1.0),
        ("scene_camera", "scene_camera", 384, 288, 1.0),
        ("scene_elements_extended", "scene_elements_extended", 800, 600, 1.0),
        ("scene_animation_config", "scene_animation_config", 192, 192, 1.0),
        ("scene_orchestration", "scene_orchestration", 800, 600, 1.0),
        ("scene_permissions", "scene_permissions", 800, 600, 1.0),
        ("scene_integration", "scene_integration", 800, 600, 1.0),
        ("scene_stress", "scene_stress", 800, 600, 1.0),
        ("text_input_ime_composition", "text_input_ime_composition", 384, 192, 1.0),
        ("text_input_grapheme_selection", "text_input_grapheme_selection", 384, 192, 1.0),
        ("text_input_fallback_cjk", "text_input_fallback_cjk", 384, 192, 1.0),
        ("progressbar", "primitive_progressbar", 192, 192, 1.0),
        ("spinner", "primitive_spinner", 192, 192, 1.0),
        ("button", "primitive_button_a8", 192, 192, 1.0),
        ("toggle", "primitive_toggle", 192, 192, 1.0),
        ("slider", "primitive_slider", 192, 192, 1.0),
        ("imageview", "image_crop_contain", 192, 192, 1.0),
        ("imageview_zoom", "image_crop_zoom", 192, 192, 1.0),
        ("nine_slice", "primitive_nine_slice", 192, 192, 1.0),
        ("text_unicode", "glyph_a8_unicode", 384, 192, 1.0),
        ("style_effects", "nested_transform_opacity_effect", 320, 240, 1.0),
        ("layer_composite", "nested_layer_composite", 320, 240, 1.0),
        ("camera_preview", "camera_preview", 192, 192, 1.0),
        ("camera_preview_legacy", "camera_preview_legacy", 192, 192, 1.0),
        ("camera_preview_bgra", "camera_preview_bgra", 192, 192, 1.0),
        ("camera_preview_blur_gray", "camera_preview_blur_gray", 192, 192, 1.0),
        ("camera_preview_tint_alpha", "camera_preview_tint_alpha", 192, 192, 1.0),
        ("camera_preview", "camera_preview_scale2", 384, 384, 2.0),
        ("camera_preview_legacy", "camera_preview_legacy_scale2", 384, 384, 2.0),
        ("camera_preview_bgra", "camera_preview_bgra_scale2", 384, 384, 2.0),
        ("camera_preview_blur_gray", "camera_preview_blur_gray_scale2", 384, 384, 2.0),
        ("camera_preview_tint_alpha", "camera_preview_tint_alpha_scale2", 384, 384, 2.0),
        ("camera_preview", "camera_preview_wide", 384, 216, 1.0),
        ("camera_preview_legacy", "camera_preview_legacy_wide", 384, 216, 1.0),
        ("camera_preview_bgra", "camera_preview_bgra_wide", 384, 216, 1.0),
        ("camera_preview_blur_gray", "camera_preview_blur_gray_wide", 384, 216, 1.0),
        ("camera_preview_tint_alpha", "camera_preview_tint_alpha_wide", 384, 216, 1.0),
        ("camera_preview", "camera_preview_portrait", 216, 384, 1.0),
        ("camera_preview_legacy", "camera_preview_legacy_portrait", 216, 384, 1.0),
        ("camera_preview_bgra", "camera_preview_bgra_portrait", 216, 384, 1.0),
        ("camera_preview_blur_gray", "camera_preview_blur_gray_portrait", 216, 384, 1.0),
        ("camera_preview_tint_alpha", "camera_preview_tint_alpha_portrait", 216, 384, 1.0),
        ("id_mask_compositor", "id_mask_compositor", 192, 192, 1.0),
        ("id_mask_compositor_city_ids", "id_mask_compositor_city_ids", 192, 192, 1.0),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids",
            192,
            192,
            1.0,
        ),
        ("id_mask_compositor_seams", "id_mask_compositor_seams", 192, 192, 1.0),
        ("id_mask_compositor", "id_mask_compositor_scale2", 384, 384, 2.0),
        ("id_mask_compositor_city_ids", "id_mask_compositor_city_ids_scale2", 384, 384, 2.0),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids_scale2",
            384,
            384,
            2.0,
        ),
        ("id_mask_compositor_seams", "id_mask_compositor_seams_scale2", 384, 384, 2.0),
        ("id_mask_compositor", "id_mask_compositor_scale3", 576, 576, 3.0),
        ("id_mask_compositor_city_ids", "id_mask_compositor_city_ids_scale3", 576, 576, 3.0),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids_scale3",
            576,
            576,
            3.0,
        ),
        ("id_mask_compositor_seams", "id_mask_compositor_seams_scale3", 576, 576, 3.0),
        ("id_mask_compositor", "id_mask_compositor_wide", 320, 192, 1.0),
        ("id_mask_compositor_city_ids", "id_mask_compositor_city_ids_wide", 320, 192, 1.0),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids_wide",
            320,
            192,
            1.0,
        ),
        ("id_mask_compositor_seams", "id_mask_compositor_seams_wide", 320, 192, 1.0),
        ("id_mask_compositor", "id_mask_compositor_wide_scale3", 960, 576, 3.0),
        (
            "id_mask_compositor_city_ids",
            "id_mask_compositor_city_ids_wide_scale3",
            960,
            576,
            3.0,
        ),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids_wide_scale3",
            960,
            576,
            3.0,
        ),
        ("id_mask_compositor_seams", "id_mask_compositor_seams_wide_scale3", 960, 576, 3.0),
        ("id_mask_compositor", "id_mask_compositor_portrait", 192, 320, 1.0),
        ("id_mask_compositor_city_ids", "id_mask_compositor_city_ids_portrait", 192, 320, 1.0),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids_portrait",
            192,
            320,
            1.0,
        ),
        ("id_mask_compositor_seams", "id_mask_compositor_seams_portrait", 192, 320, 1.0),
        ("id_mask_compositor", "id_mask_compositor_portrait_scale3", 576, 960, 3.0),
        (
            "id_mask_compositor_city_ids",
            "id_mask_compositor_city_ids_portrait_scale3",
            576,
            960,
            3.0,
        ),
        (
            "id_mask_compositor_neighborhood_ids",
            "id_mask_compositor_neighborhood_ids_portrait_scale3",
            576,
            960,
            3.0,
        ),
        (
            "id_mask_compositor_seams",
            "id_mask_compositor_seams_portrait_scale3",
            576,
            960,
            3.0,
        ),
    ];
    let dir = temp_case_dir("committed-renderer");

    for (component, golden_name, width, height, scale) in cases {
        let golden = golden_dir.join(format!("{golden_name}.png"));
        assert!(golden.exists(), "missing committed golden {}", golden.display());
        let out = dir.join(format!("{golden_name}.png"));
        let pixel_tolerance = if golden_name == "nested_layer_composite" { 96 } else { 16 };
        let output =
            run_snapshot_checked(
                component,
                width,
                height,
                scale,
                &out,
                &golden,
                pixel_tolerance,
                None,
            );
        assert!(
            output.status.success(),
            "committed golden mismatch for {golden_name}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for (component, golden_name) in [
        ("style_effects", "nested_transform_opacity_effect"),
        ("layer_composite", "nested_layer_composite"),
    ] {
        let golden = golden_dir.join(format!("{golden_name}.png"));
        let out = dir.join(format!("{golden_name}-inline-reference.png"));
        let output = run_snapshot_checked(component, 320, 240, 1.0, &out, &golden, 16, Some("0"));
        assert!(
            output.status.success(),
            "inline layer reference mismatch for {golden_name}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for name in [
        "id_mask_compositor_seams.png",
        "id_mask_compositor_seams_scale2.png",
        "id_mask_compositor_seams_wide.png",
        "id_mask_compositor_seams_portrait.png",
        "id_mask_compositor_seams_scale3.png",
        "id_mask_compositor_seams_wide_scale3.png",
        "id_mask_compositor_seams_portrait_scale3.png",
    ] {
        let (bright, dark) = png_signal_counts(&golden_dir.join(name));
        assert!(
            bright > 300 && dark > bright * 10,
            "seam golden {name} lost high-contrast signal: bright={bright} dark={dark}"
        );
    }

    for name in [
        "id_mask_compositor.png",
        "id_mask_compositor_city_ids.png",
        "id_mask_compositor_neighborhood_ids.png",
        "id_mask_compositor_scale2.png",
        "id_mask_compositor_city_ids_scale2.png",
        "id_mask_compositor_neighborhood_ids_scale2.png",
        "id_mask_compositor_wide.png",
        "id_mask_compositor_city_ids_wide.png",
        "id_mask_compositor_neighborhood_ids_wide.png",
        "id_mask_compositor_portrait.png",
        "id_mask_compositor_city_ids_portrait.png",
        "id_mask_compositor_neighborhood_ids_portrait.png",
        "id_mask_compositor_scale3.png",
        "id_mask_compositor_city_ids_scale3.png",
        "id_mask_compositor_neighborhood_ids_scale3.png",
        "id_mask_compositor_wide_scale3.png",
        "id_mask_compositor_city_ids_wide_scale3.png",
        "id_mask_compositor_neighborhood_ids_wide_scale3.png",
        "id_mask_compositor_portrait_scale3.png",
        "id_mask_compositor_city_ids_portrait_scale3.png",
        "id_mask_compositor_neighborhood_ids_portrait_scale3.png",
    ] {
        let color_signal = png_color_signal_count(&golden_dir.join(name));
        assert!(
            color_signal > 1_000,
            "id-mask golden {name} lost colored region signal: color_signal={color_signal}"
        );
    }

    for name in [
        "scene3d_mixed.png",
        "scene3d_bloom.png",
        "scene3d_depth_stack.png",
        "scene3d_viewport_clip.png",
        "scene3d_material_cull.png",
        "scene3d_blend_modes.png",
        "scene3d_mixed_scale2.png",
        "scene3d_bloom_scale2.png",
        "scene3d_depth_stack_scale2.png",
        "scene3d_viewport_clip_scale2.png",
        "scene3d_material_cull_scale2.png",
        "scene3d_blend_modes_scale2.png",
        "scene3d_mixed_scale3.png",
        "scene3d_bloom_scale3.png",
        "scene3d_depth_stack_scale3.png",
        "scene3d_viewport_clip_scale3.png",
        "scene3d_material_cull_scale3.png",
        "scene3d_blend_modes_scale3.png",
        "scene3d_mixed_wide.png",
        "scene3d_bloom_wide.png",
        "scene3d_depth_stack_wide.png",
        "scene3d_viewport_clip_wide.png",
        "scene3d_material_cull_wide.png",
        "scene3d_blend_modes_wide.png",
        "scene3d_mixed_portrait.png",
        "scene3d_bloom_portrait.png",
        "scene3d_depth_stack_portrait.png",
        "scene3d_viewport_clip_portrait.png",
        "scene3d_material_cull_portrait.png",
        "scene3d_blend_modes_portrait.png",
    ] {
        let color_signal = png_color_signal_count(&golden_dir.join(name));
        assert!(
            color_signal > 200,
            "scene3d golden {name} lost colored geometry signal: color_signal={color_signal}"
        );
    }

    for name in [
        "camera_preview.png",
        "camera_preview_legacy.png",
        "camera_preview_bgra.png",
        "camera_preview_blur_gray.png",
        "camera_preview_tint_alpha.png",
        "camera_preview_scale2.png",
        "camera_preview_legacy_scale2.png",
        "camera_preview_bgra_scale2.png",
        "camera_preview_blur_gray_scale2.png",
        "camera_preview_tint_alpha_scale2.png",
        "camera_preview_wide.png",
        "camera_preview_legacy_wide.png",
        "camera_preview_bgra_wide.png",
        "camera_preview_blur_gray_wide.png",
        "camera_preview_tint_alpha_wide.png",
        "camera_preview_portrait.png",
        "camera_preview_legacy_portrait.png",
        "camera_preview_bgra_portrait.png",
        "camera_preview_blur_gray_portrait.png",
        "camera_preview_tint_alpha_portrait.png",
    ] {
        let non_white = png_non_white_count(&golden_dir.join(name));
        assert!(
            non_white > 1_000,
            "camera golden {name} lost visible preview signal: non_white={non_white}"
        );
    }

    for name in [
        "scene_anim_timeline.png",
        "scene_input_lab.png",
        "scene_nine_slice.png",
        "scene_sdf_text.png",
        "scene_snapshot.png",
        "scene_camera.png",
        "scene_elements_extended.png",
        "scene_animation_config.png",
        "scene_orchestration.png",
        "scene_permissions.png",
        "scene_integration.png",
        "scene_stress.png",
        "text_input_ime_composition.png",
        "text_input_grapheme_selection.png",
        "text_input_fallback_cjk.png",
    ] {
        let non_white = png_non_white_count(&golden_dir.join(name));
        assert!(
            non_white > 900,
            "router-scene golden {name} lost visible scene signal: non_white={non_white}"
        );
    }

    for name in [
        "primitive_progressbar.png",
        "primitive_spinner.png",
        "primitive_button_a8.png",
        "primitive_toggle.png",
        "primitive_slider.png",
        "image_crop_contain.png",
        "image_crop_zoom.png",
        "primitive_nine_slice.png",
        "glyph_a8_unicode.png",
        "nested_transform_opacity_effect.png",
        "nested_layer_composite.png",
    ] {
        let non_white = png_non_white_count(&golden_dir.join(name));
        assert!(non_white > 128, "renderer golden {name} lost visible signal: non_white={non_white}");
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn committed_webgpu_browser_golden_exists_with_expected_size() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let golden_dir = manifest.join("../..").join("goldens/snapshots");
    let cases = [
        ("webgpu_browser.png", (320_u32, 240_u32)),
        ("webgpu_browser_wide.png", (640, 360)),
        ("webgpu_browser_portrait.png", (360, 640)),
        ("webgpu_scene3d.png", (512, 512)),
        ("webgpu_scene3d_wide.png", (640, 360)),
        ("webgpu_scene3d_portrait.png", (360, 640)),
        ("webgpu_id_mask_compositor.png", (512, 512)),
        ("webgpu_id_mask_compositor_wide.png", (640, 360)),
        ("webgpu_id_mask_compositor_portrait.png", (360, 640)),
    ];

    for (name, size) in cases {
        let golden = golden_dir.join(name);
        assert!(golden.exists(), "missing committed golden {}", golden.display());
        assert_eq!(png_size(&golden), size);
    }

    for name in ["webgpu_browser.png", "webgpu_browser_wide.png", "webgpu_browser_portrait.png"] {
        let non_white = png_non_white_count(&golden_dir.join(name));
        assert!(
            non_white > 10_000,
            "webgpu app golden {name} lost visible scene signal: non_white={non_white}"
        );
    }

    for name in ["webgpu_scene3d.png", "webgpu_scene3d_wide.png", "webgpu_scene3d_portrait.png"] {
        let scene3d_color = png_color_signal_count(&golden_dir.join(name));
        assert!(
            scene3d_color > 20_000,
            "webgpu Scene3D golden {name} lost colored geometry signal: color_signal={scene3d_color}"
        );
    }

    for name in [
        "webgpu_id_mask_compositor.png",
        "webgpu_id_mask_compositor_wide.png",
        "webgpu_id_mask_compositor_portrait.png",
    ] {
        let non_white = png_non_white_count(&golden_dir.join(name));
        assert!(
            non_white > 1_000,
            "webgpu id-mask golden {name} lost visible compositor signal: non_white={non_white}"
        );
    }
}

#[test]
fn allow_mismatch_keeps_pixel_and_size_mismatch_explicit() {
    let dir = temp_case_dir("allow");
    let golden = create_button_golden(&dir);
    let pixel_out = dir.join("spinner-allowed.png");
    let size_out = dir.join("button-resized-allowed.png");

    let pixel = run_snapshot("spinner", 160, 120, 1.0, &pixel_out, &golden, true);
    let size = run_snapshot("button", 168, 120, 1.0, &size_out, &golden, true);

    assert!(
        pixel.status.success(),
        "allowed pixel mismatch failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&pixel.stdout),
        String::from_utf8_lossy(&pixel.stderr)
    );
    assert!(
        String::from_utf8_lossy(&pixel.stdout).contains("pixdiff="),
        "allowed pixel mismatch should still report diff summary"
    );
    assert!(
        size.status.success(),
        "allowed size mismatch failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&size.stdout),
        String::from_utf8_lossy(&size.stderr)
    );
    assert!(
        String::from_utf8_lossy(&size.stderr).contains("golden size mismatch"),
        "allowed size mismatch should still print diagnostic stderr"
    );
    let _ = fs::remove_dir_all(dir);
}
