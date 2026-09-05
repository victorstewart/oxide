use std::{
   fs,
   path::PathBuf,
   time::{SystemTime, UNIX_EPOCH},
};

use oxide_text_compositing_diagnostic::{diagnostic_request, run, sha256, CANVAS_HEIGHT, CANVAS_WIDTH, DEVICE_SCALE, FONT_SHA256};

#[test]
fn request_freezes_two_titles_and_all_known_signatures()
{
   let request = diagnostic_request();
   assert_eq!(request.schema_version, 1);
   assert_eq!((request.canvas_width, request.canvas_height, request.device_scale), (CANVAS_WIDTH, CANVAS_HEIGHT, DEVICE_SCALE));
   assert_eq!(request.font_sha256, FONT_SHA256);
   assert_eq!(request.background_srgb, [243, 245, 248]);
   assert_eq!(request.text_srgb, [32, 36, 44]);
   assert_eq!(request.cases.len(), 2);
   assert_eq!(request.cases[0].id, "navigation");
   assert_eq!(request.cases[0].signatures.len(), 16);
   assert_eq!(request.cases[1].id, "image");
   assert_eq!(request.cases[1].signatures.len(), 19);
   assert_eq!(request.cases.iter().map(|case| case.signatures.len()).sum::<usize>(), 35);
}

#[test]
fn pinned_font_identity_is_frozen()
{
   let font = fs::read(font_path()).expect("pinned font must be readable");
   assert_eq!(sha256(&font), FONT_SHA256);
}

#[test]
fn swift_helper_keeps_all_three_paths_offscreen_and_bounded()
{
   let source = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("TextCompositingReference.swift")).expect("Swift helper must be readable");
   assert!(source.contains("CTFontDrawGlyphs"));
   assert!(source.contains("alphaOnly"));
   assert!(source.contains("renderDirect"));
   assert!(source.contains("renderMaskFill"));
   assert!(source.contains("renderMetalCPU"));
   assert!(!source.contains("NSApplication"));
   assert!(!source.contains("NSWindow"));
}

#[test]
fn metal_helper_freezes_the_production_atlas_pipeline_stages()
{
   let source = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("MetalTextProbe.swift")).expect("Metal helper must be readable");
   assert!(source.contains("import Metal"));
   assert!(source.contains(".r8Unorm"));
   assert!(source.contains(".r32Float"));
   assert!(source.contains(".bgra8Unorm_srgb"));
   assert!(source.contains("descriptor.minFilter = linear ? .linear : .nearest"));
   assert!(source.contains("attachment.sourceRGBBlendFactor = .sourceAlpha"));
   assert!(source.contains("attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha"));
   assert!(source.contains("probe_coverage"));
   assert!(source.contains("probe_glyph"));
   assert!(!source.contains("CAMetalLayer"));
   assert!(!source.contains("NSApplication"));
}

#[cfg(target_os = "macos")]
#[test]
fn helper_persists_complete_identity_checked_report()
{
   let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock must follow epoch").as_nanos();
   let output = std::env::temp_dir().join(format!("oxide-text-compositing-{}-{nonce}", std::process::id()));
   let summary = run(&font_path(), &output).expect("offscreen diagnostic must complete");
   assert_eq!(summary.schema_version, 1);
   assert_eq!(summary.case_count, 2);
   assert_eq!(summary.signature_count, 35);
   assert_eq!(summary.direct_native_match_count, 30);
   assert_eq!(summary.mask_fill_native_match_count, 30);
   assert_eq!(summary.metal_oxide_match_count, 5);
   assert_eq!(summary.cases.iter().map(|case| case.signature_count).sum::<usize>(), 35);
   assert_eq!(summary.cases[0].a8_sha256, "ffcfea7d4c7cd844a7e49b91d8ac1d4e18a83b10e3cdd4d6de65949937de4729");
   assert_eq!(summary.cases[0].direct_rgb_sha256, "a5be0652c49f70f47ef7a79825f14645dc7cd4ed67d1dc0563fe56df61b9a2d9");
   assert_eq!(summary.cases[0].direct_rgb_sha256, summary.cases[0].mask_fill_rgb_sha256);
   assert_eq!(summary.cases[0].metal_cpu_rgb_sha256, "687c0f9288e176d816bc8869af668e9745dfe98d244e03bf8e06040180a09cb3");
   assert_eq!(summary.cases[1].a8_sha256, "d4fc7a761b0c00e7268365402b0c5565d78a2bdbc30f9907f3d561f5ecb7b17c");
   assert_eq!(summary.cases[1].direct_rgb_sha256, "f1eac519edd2fa4a9f521b03007274fcb849169f8e312b250010f133a65ecc3a");
   assert_eq!(summary.cases[1].direct_rgb_sha256, summary.cases[1].mask_fill_rgb_sha256);
   assert_eq!(summary.cases[1].metal_cpu_rgb_sha256, "0565f6e8a96eea2350639f53058383d1e75aeed8e81a94d94983e59d78af3d51");
   let metal = summary.metal_probe.as_ref().expect("complete diagnostic must include the Metal stage report");
   assert_eq!(metal.signature_count, 35);
   assert_eq!(metal.point_cpu_oxide_match_count, 5);
   assert_eq!(metal.linear_cpu_oxide_match_count, 5);
   assert_eq!(metal.metal_full_oxide_match_count, 35);
   assert_eq!(metal.earliest_scalar_a8_replay_count, 5);
   assert_eq!(metal.earliest_atlas_raster_and_geometry_count, 0);
   assert_eq!(metal.earliest_linear_sampler_count, 0);
   assert_eq!(metal.earliest_fixed_blend_srgb_target_count, 30);
   assert_eq!(metal.unexplained_count, 0);
   assert!(metal.cases.iter().all(|case| case.scalar_vs_point_differing_pixel_count == 0));
   assert!(metal.cases.iter().all(|case| case.point_vs_linear_differing_pixel_count == 0));
   assert_eq!(metal.cases[0].atlas_sha256, "c6c58c0e11544168432d58e9f67753323d7d887f0be4acf950d0776b0be7cddb");
   assert_eq!(metal.cases[0].metal_full_rgb_sha256, "a7b6e99afd0048f0b022594015087ca706217e18b117b60bb188caa0997d8be7");
   assert_eq!(metal.cases[1].atlas_sha256, "534a809a96b115e2a47e9d5c9b7481556fefd71dfb9d5d08ef5d35e73e93421e");
   assert_eq!(metal.cases[1].metal_full_rgb_sha256, "39d1420fa6302215975a13f206bf87a915707eb7cb1ebffffb17e4249efc639d");
   fs::remove_dir_all(&output).expect("bounded diagnostic output must be removable");
}

fn font_path() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1/NotoSans-VF.ttf")
}
