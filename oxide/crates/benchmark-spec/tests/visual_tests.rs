use std::io::Cursor;

use oxide_benchmark_spec::{
   compare_calibrated_static_pngs, compare_exact_static_pngs,
   reduce_normalized_png_visual_parity, CalibratedStaticVisualThresholds, LogicalRect,
   PhysicalRect, TextGeometryEvidence, TextLineGeometry, TextValidationStatus,
   VisualThresholds, CALIBRATED_STATIC_VISUAL_ALGORITHM, EXACT_STATIC_VISUAL_ALGORITHM,
   NORMALIZED_PNG_VISUAL_ALGORITHM,
};

fn rgb_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8>
{
   let mut encoded = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut encoded, width, height);
      encoder.set_color(png::ColorType::Rgb);
      encoder.set_depth(png::BitDepth::Eight);
      encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
      let mut writer = encoder.write_header().expect("write PNG header");
      writer.write_image_data(pixels).expect("write PNG pixels");
      writer.finish().expect("finish PNG");
   }
   encoded
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8>
{
   let mut encoded = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut encoded, width, height);
      encoder.set_color(png::ColorType::Rgba);
      encoder.set_depth(png::BitDepth::Eight);
      encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
      let mut writer = encoder.write_header().expect("write PNG header");
      writer.write_image_data(pixels).expect("write PNG pixels");
      writer.finish().expect("finish PNG");
   }
   encoded
}

fn pixels(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8>
{
   let mut result = Vec::with_capacity(width as usize * height as usize * 3);
   for _ in 0..width as usize * height as usize
   {
      result.extend_from_slice(&rgb);
   }
   result
}

fn layout(width: u32, height: u32, masks: &str) -> Vec<u8>
{
   format!(r#"{{"coordinate_space":"logical-points","root":{{"x":0,"y":0,"width":{width},"height":{height}}},"text_comparison":{{"pixel_mask":true,"baseline_tolerance":0.5,"ink_bounds_tolerance":0.5}},"text_masks":{masks}}}"#).into_bytes()
}

fn matching_text() -> TextGeometryEvidence
{
   let line = TextLineGeometry {
      baseline_y: 2.0,
      ink_bounds: LogicalRect { x: 0.5, y: 0.5, width: 1.0, height: 1.0 },
   };
   TextGeometryEvidence { reference_lines: vec![line], candidate_lines: vec![line] }
}

#[test]
fn identical_normalized_pngs_pass_frozen_thresholds_and_scale_masks()
{
   let image = rgb_png(16, 16, &pixels(16, 16, [64, 128, 192]));
   let report = reduce_normalized_png_visual_parity(&image, &image, &layout(8, 8, "[[0.5,0.5,1.0,1.0]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect("reduce identical PNGs");

   assert_eq!(report.algorithm, NORMALIZED_PNG_VISUAL_ALGORITHM);
   assert_eq!(report.text_masks, vec![PhysicalRect { x: 1, y: 1, width: 2, height: 2 }]);
   assert_eq!(report.masked_pixel_count, 4);
   assert_eq!(report.non_text.compared_pixel_count, 252);
   assert_eq!(report.non_text.differing_pixel_count, 0);
   assert_eq!(report.non_text.windowed_ssim, 1.0);
   assert_eq!(report.non_text.median_delta_e, 0.0);
   assert_eq!(report.non_text.p99_delta_e, 0.0);
   assert!(report.non_text_accepted);
   assert!(matches!(report.text_validation, TextValidationStatus::Validated { line_count: 1, .. }));
   assert!(report.accepted);
}

#[test]
fn changes_inside_logical_text_mask_do_not_affect_non_text_metrics()
{
   let reference_pixels = pixels(16, 16, [32, 64, 96]);
   let mut candidate_pixels = reference_pixels.clone();
   for y in 1..3
   {
      for x in 1..3
      {
         let offset = (y * 16 + x) * 3;
         candidate_pixels[offset..offset + 3].copy_from_slice(&[255, 0, 255]);
      }
   }
   let reference = rgb_png(16, 16, &reference_pixels);
   let candidate = rgb_png(16, 16, &candidate_pixels);
   let report = reduce_normalized_png_visual_parity(&reference, &candidate, &layout(8, 8, "[[0.5,0.5,1.0,1.0]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect("reduce text-masked PNGs");

   assert_eq!(report.non_text.differing_pixel_count, 0);
   assert!(report.accepted);
}

#[test]
fn visual_fidelity_degradation_is_rejected()
{
   let reference_pixels = pixels(64, 64, [120, 130, 140]);
   let mut candidate_pixels = reference_pixels.clone();
   for y in 24..40
   {
      for x in 24..40
      {
         let offset = (y * 64 + x) * 3;
         candidate_pixels[offset..offset + 3].copy_from_slice(&[0, 0, 0]);
      }
   }
   let reference = rgb_png(64, 64, &reference_pixels);
   let candidate = rgb_png(64, 64, &candidate_pixels);
   let report = reduce_normalized_png_visual_parity(&reference, &candidate, &layout(32, 32, "[[1,1,2,2]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect("reduce degraded PNG");

   assert!(report.non_text.differing_pixel_ratio > 0.005);
   assert!(!report.non_text_accepted);
   assert!(!report.accepted);
}

#[test]
fn missing_text_geometry_is_explicitly_pending_and_fail_closed()
{
   let image = rgb_png(16, 16, &pixels(16, 16, [64, 64, 64]));
   let report = reduce_normalized_png_visual_parity(&image, &image, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2, None, VisualThresholds::default()).expect("reduce without text evidence");

   assert!(report.non_text_accepted);
   assert!(matches!(report.text_validation, TextValidationStatus::Pending { .. }));
   assert!(!report.accepted);
}

#[test]
fn missing_layout_masks_remain_pending_even_with_text_geometry()
{
   let image = rgb_png(16, 16, &pixels(16, 16, [64, 64, 64]));
   let report = reduce_normalized_png_visual_parity(&image, &image, &layout(8, 8, "[]"), 2, Some(&matching_text()), VisualThresholds::default()).expect("reduce without layout masks");

   assert!(report.non_text_accepted);
   assert!(matches!(report.text_validation, TextValidationStatus::Pending { ref reason } if reason.contains("provides no logical-point text_masks")));
   assert!(!report.accepted);
}

#[test]
fn mismatched_text_baseline_is_rejected()
{
   let image = rgb_png(16, 16, &pixels(16, 16, [64, 64, 64]));
   let mut evidence = matching_text();
   evidence.candidate_lines[0].baseline_y = 2.75;
   let report = reduce_normalized_png_visual_parity(&image, &image, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2, Some(&evidence), VisualThresholds::default()).expect("reduce text mismatch");

   assert!(matches!(report.text_validation, TextValidationStatus::Rejected { maximum_baseline_delta_points: Some(delta), .. } if delta == 0.75));
   assert!(!report.accepted);
}

#[test]
fn exact_candidate_and_canonical_dimensions_are_required()
{
   let image_16 = rgb_png(16, 16, &pixels(16, 16, [0, 0, 0]));
   let image_18 = rgb_png(18, 16, &pixels(18, 16, [0, 0, 0]));
   let mismatch = reduce_normalized_png_visual_parity(&image_16, &image_18, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect_err("reject mismatched PNG dimensions");
   assert!(mismatch.to_string().contains("dimensions differ"));

   let wrong_canonical = reduce_normalized_png_visual_parity(&image_16, &image_16, &layout(9, 8, "[[0.5,0.5,1,1]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect_err("reject wrong canonical dimensions");
   assert!(wrong_canonical.to_string().contains("expected exact canonical dimensions"));
}

#[test]
fn nonopaque_pngs_are_rejected_instead_of_assuming_a_backdrop()
{
   let mut rgba = vec![32u8; 16 * 16 * 4];
   for pixel in rgba.chunks_exact_mut(4)
   {
      pixel[3] = 255;
   }
   rgba[3] = 254;
   let image = rgba_png(16, 16, &rgba);
   let error = reduce_normalized_png_visual_parity(&image, &image, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect_err("reject nonopaque PNG");

   assert!(error.to_string().contains("nonopaque pixels"));
}

#[test]
fn untagged_pngs_are_rejected_instead_of_assuming_srgb()
{
   let mut encoded = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut encoded, 16, 16);
      encoder.set_color(png::ColorType::Rgb);
      encoder.set_depth(png::BitDepth::Eight);
      let mut writer = encoder.write_header().expect("write untagged PNG header");
      writer.write_image_data(&pixels(16, 16, [64, 64, 64])).expect("write untagged PNG pixels");
      writer.finish().expect("finish untagged PNG");
   }
   let tagged = rgb_png(16, 16, &pixels(16, 16, [64, 64, 64]));
   let error = compare_exact_static_pngs(&encoded, &tagged, &layout(8, 8, "[]"), 2).expect_err("reject ambiguous color encoding");

   assert!(error.to_string().contains("does not declare the sRGB color space"));
}

#[test]
fn default_thresholds_freeze_section_four_contract()
{
   let thresholds = VisualThresholds::default();

   assert_eq!(thresholds.minimum_non_text_ssim, 0.995);
   assert_eq!(thresholds.maximum_differing_pixel_ratio, 0.005);
   assert_eq!(thresholds.differing_channel_tolerance, 2);
   assert_eq!(thresholds.maximum_median_delta_e, 1.0);
   assert_eq!(thresholds.maximum_p99_delta_e, 3.0);
}

#[test]
fn calibrated_thresholds_freeze_rapid_visual_contract()
{
   let thresholds = CalibratedStaticVisualThresholds::default();

   assert_eq!(thresholds.minimum_full_frame_ssim, 0.9795);
   assert_eq!(thresholds.maximum_stable_interior_differing_pixel_ratio, 0.03);
   assert_eq!(thresholds.differing_channel_tolerance, 2);
   assert_eq!(thresholds.voxel_edge_pixels, 16);
   assert_eq!(thresholds.maximum_voxel_mean_absolute_channel_delta, 45.0);
}

#[test]
fn report_json_is_byte_deterministic_for_identical_inputs()
{
   let image = rgb_png(16, 16, &pixels(16, 16, [72, 96, 120]));
   let reduce = || reduce_normalized_png_visual_parity(&image, &image, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2, Some(&matching_text()), VisualThresholds::default()).expect("reduce PNGs");
   let first = serde_json::to_vec(&reduce()).expect("serialize first report");
   let second = serde_json::to_vec(&reduce()).expect("serialize second report");

   assert_eq!(Cursor::new(first).into_inner(), Cursor::new(second).into_inner());
}

#[test]
fn exact_static_comparison_requires_every_oxide_and_uikit_pixel_to_match()
{
   let oxide_pixels = pixels(16, 16, [72, 96, 120]);
   let mut uikit_pixels = oxide_pixels.clone();
   let oxide = rgb_png(16, 16, &oxide_pixels);
   let identical = rgba_png(16, 16, &oxide_pixels.chunks_exact(3)
      .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
      .collect::<Vec<_>>());
   let accepted = compare_exact_static_pngs(&oxide, &identical, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2).expect("compare identical static pixels");
   assert_eq!(accepted.algorithm, EXACT_STATIC_VISUAL_ALGORITHM);
   assert_eq!(accepted.oxide_png_sha256.len(), 64);
   assert_eq!(accepted.uikit_png_sha256.len(), 64);
   assert_eq!(accepted.layout_json_sha256.len(), 64);
   assert_ne!(accepted.oxide_png_sha256, accepted.uikit_png_sha256);
   assert_eq!(accepted.compared_pixel_count, 256);
   assert_eq!(accepted.differing_pixel_count, 0);
   assert_eq!(accepted.differing_channel_count, 0);
   assert_eq!(accepted.maximum_channel_delta, 0);
   assert_eq!(accepted.differing_bounds, None);
   assert!(accepted.channel_delta_histogram.is_empty());
   assert!(accepted.accepted);

   uikit_pixels[(2 * 16 + 2) * 3 + 1] += 1;
   let uikit = rgb_png(16, 16, &uikit_pixels);
   let rejected = compare_exact_static_pngs(&oxide, &uikit, &layout(8, 8, "[[0.5,0.5,1,1]]"), 2).expect("compare one-channel static mismatch");
   assert_eq!(rejected.oxide_png_sha256, accepted.oxide_png_sha256);
   assert_ne!(rejected.uikit_png_sha256, accepted.uikit_png_sha256);
   assert_eq!(rejected.layout_json_sha256, accepted.layout_json_sha256);
   assert_eq!(rejected.differing_pixel_count, 1);
   assert_eq!(rejected.differing_channel_count, 1);
   assert_eq!(rejected.maximum_channel_delta, 1);
   assert_eq!(rejected.differing_bounds, Some(PhysicalRect {x: 2, y: 2, width: 1, height: 1}));
   assert_eq!(rejected.channel_delta_histogram.get(&1), Some(&1));
   assert!(!rejected.accepted);
}

#[test]
fn exact_static_comparison_accepts_live_swift_camel_case_coordinate_space()
{
   let image = rgb_png(16, 16, &pixels(16, 16, [72, 96, 120]));
   let live_geometry = br#"{"canonicalScale":2,"captureProfile":"macos","coordinateSpace":"logical-points","nodes":[],"positionTolerancePoints":0,"root":{"x":0,"y":0,"width":8,"height":8},"schemaVersion":1,"sizeTolerancePoints":0}"#;
   let report = compare_exact_static_pngs(&image, &image, live_geometry, 2).expect("compare using live Swift geometry");

   assert!(report.accepted);
}

#[test]
fn calibrated_static_comparison_accepts_sparse_raster_noise_but_keeps_exact_diagnostics()
{
   let reference_pixels = pixels(64, 64, [72, 96, 120]);
   let mut candidate_pixels = reference_pixels.clone();
   candidate_pixels[(31 * 64 + 31) * 3] = 255;
   let reference = rgb_png(64, 64, &reference_pixels);
   let candidate = rgb_png(64, 64, &candidate_pixels);
   let report = compare_calibrated_static_pngs(&reference, &candidate, &layout(32, 32, "[]"), 2, CalibratedStaticVisualThresholds::default()).expect("compare sparse raster noise");

   assert_eq!(report.algorithm, CALIBRATED_STATIC_VISUAL_ALGORITHM);
   assert!(report.accepted);
   assert_eq!(report.full_frame.differing_pixel_count, 1);
   assert_eq!(report.stable_interior.differing_pixel_count, 1);
   assert_eq!(report.exact_diagnostic.differing_pixel_count, 1);
   assert!(!report.exact_diagnostic.accepted);
}

#[test]
fn calibrated_static_report_is_byte_deterministic()
{
   let image = rgb_png(32, 32, &pixels(32, 32, [72, 96, 120]));
   let geometry = br#"{"coordinate_space":"logical-points","root":{"x":0,"y":0,"width":16,"height":16},"nodes":[]}"#;
   let compare = || compare_calibrated_static_pngs(&image, &image, geometry, 2, CalibratedStaticVisualThresholds::default()).expect("compare identical calibrated PNGs");
   let first = serde_json::to_vec(&compare()).expect("serialize first calibrated report");
   let second = serde_json::to_vec(&compare()).expect("serialize second calibrated report");

   assert_eq!(first, second);
}

#[test]
fn calibrated_static_comparison_rejects_invalid_thresholds()
{
   let image = rgb_png(32, 32, &pixels(32, 32, [72, 96, 120]));
   let geometry = br#"{"coordinate_space":"logical-points","root":{"x":0,"y":0,"width":16,"height":16},"nodes":[]}"#;
   let mut thresholds = CalibratedStaticVisualThresholds::default();
   thresholds.minimum_full_frame_ssim = f64::NAN;
   let error = compare_calibrated_static_pngs(&image, &image, geometry, 2, thresholds).expect_err("reject invalid calibrated thresholds");

   assert!(error.to_string().contains("minimum full-frame SSIM"));
}

#[test]
fn calibrated_static_text_regions_cannot_hide_a_wrong_component_interior()
{
   let reference_pixels = pixels(32, 32, [240, 240, 240]);
   let mut candidate_pixels = reference_pixels.clone();
   for y in 0..16
   {
      for x in 0..16
      {
         let offset = (y * 32 + x) * 3;
         candidate_pixels[offset..offset + 3].copy_from_slice(&[24, 28, 36]);
      }
   }
   let reference = rgb_png(32, 32, &reference_pixels);
   let candidate = rgb_png(32, 32, &candidate_pixels);
   let geometry = br#"{"coordinate_space":"logical-points","root":{"x":0,"y":0,"width":16,"height":16},"nodes":[{"role":"label","bounds":{"x":0,"y":0,"width":8,"height":8},"textLineBounds":[{"x":2,"y":3,"width":4,"height":2}]}]}"#;
   let report = compare_calibrated_static_pngs(&reference, &candidate, geometry, 2, CalibratedStaticVisualThresholds::default()).expect("compare text-bearing component interior");

   assert_eq!(report.text_regions, vec![PhysicalRect {x: 3, y: 5, width: 10, height: 6}]);
   assert_eq!(report.text_region_pixel_count, 60);
   assert_eq!(report.full_frame.compared_pixel_count, 1024);
   assert_eq!(report.stable_interior.compared_pixel_count, 964);
   assert_eq!(report.full_frame.differing_pixel_count, 256);
   assert!(report.stable_interior.differing_pixel_count > 0);
   assert_eq!(report.exact_diagnostic.differing_pixel_count, 256);
   assert!(!report.accepted);
}

#[test]
fn calibrated_static_text_regions_do_not_hide_missing_text_pixels()
{
   let mut reference_pixels = pixels(64, 64, [240, 240, 240]);
   let candidate_pixels = reference_pixels.clone();
   for y in 8..24
   {
      for x in 8..24
      {
         let offset = (y * 64 + x) * 3;
         reference_pixels[offset..offset + 3].copy_from_slice(&[24, 28, 36]);
      }
   }
   let reference = rgb_png(64, 64, &reference_pixels);
   let candidate = rgb_png(64, 64, &candidate_pixels);
   let geometry = br#"{"coordinate_space":"logical-points","root":{"x":0,"y":0,"width":32,"height":32},"nodes":[{"role":"label","bounds":{"x":4,"y":4,"width":8,"height":8},"textLineBounds":[{"x":4,"y":4,"width":8,"height":8}]}]}"#;
   let report = compare_calibrated_static_pngs(&reference, &candidate, geometry, 2, CalibratedStaticVisualThresholds::default()).expect("compare missing text raster");

   assert_eq!(report.stable_interior.differing_pixel_count, 0);
   assert_eq!(report.full_frame.differing_pixel_count, 256);
   assert!(report.voxels.maximum_mean_absolute_channel_delta > report.thresholds.maximum_voxel_mean_absolute_channel_delta);
   assert!(!report.accepted);
}

#[test]
fn calibrated_static_rounded_surface_pixels_remain_inside_every_acceptance_guard()
{
   let reference_pixels = pixels(64, 64, [240, 240, 240]);
   let mut candidate_pixels = reference_pixels.clone();
   for y in 14..18
   {
      for x in 14..18
      {
         let offset = (y * 64 + x) * 3;
         candidate_pixels[offset..offset + 3].copy_from_slice(&[238, 238, 238]);
      }
   }
   let reference = rgb_png(64, 64, &reference_pixels);
   let candidate = rgb_png(64, 64, &candidate_pixels);
   let geometry = br#"{"coordinate_space":"logical-points","root":{"x":0,"y":0,"width":32,"height":32},"nodes":[{"role":"feed-card","bounds":{"x":8,"y":8,"width":16,"height":16}}]}"#;
   let report = compare_calibrated_static_pngs(&reference, &candidate, geometry, 2, CalibratedStaticVisualThresholds::default()).expect("compare rounded surface edge rasterization");

   assert!(report.accepted);
   assert_eq!(report.text_region_pixel_count, 0);
   assert_eq!(report.full_frame.differing_pixel_count, 0);
   assert_eq!(report.stable_interior.differing_pixel_count, 0);
   assert!(report.voxels.maximum_mean_absolute_channel_delta > 0.0);
   assert_eq!(report.exact_diagnostic.differing_pixel_count, 16);
}

#[test]
fn calibrated_static_voxel_averaging_cancels_within_voxel_raster_redistribution()
{
   let mut reference_pixels = pixels(32, 32, [240, 240, 240]);
   let mut candidate_pixels = reference_pixels.clone();
   for y in 4..12
   {
      for x in 4..8
      {
         let reference_offset = (y * 32 + x) * 3;
         let candidate_offset = (y * 32 + x + 4) * 3;
         reference_pixels[reference_offset..reference_offset + 3].copy_from_slice(&[40, 44, 52]);
         candidate_pixels[candidate_offset..candidate_offset + 3].copy_from_slice(&[40, 44, 52]);
      }
   }
   let reference = rgb_png(32, 32, &reference_pixels);
   let candidate = rgb_png(32, 32, &candidate_pixels);
   let mut thresholds = CalibratedStaticVisualThresholds::default();
   thresholds.minimum_full_frame_ssim = 0.0;
   thresholds.maximum_stable_interior_differing_pixel_ratio = 1.0;
   thresholds.maximum_voxel_mean_absolute_channel_delta = 0.0;
   let report = compare_calibrated_static_pngs(&reference, &candidate, &layout(16, 16, "[]"), 2, thresholds).expect("compare redistributed raster coverage");

   assert!(report.full_frame.differing_pixel_count > 0);
   assert_eq!(report.voxels.maximum_mean_absolute_channel_delta, 0.0);
   assert!(report.accepted);
}

#[test]
fn calibrated_static_voxel_guard_rejects_a_half_sized_visual_defect_hidden_by_global_averages()
{
   let mut reference_pixels = pixels(256, 256, [240, 240, 240]);
   let mut candidate_pixels = reference_pixels.clone();
   for y in 120..136
   {
      for x in 120..136
      {
         let offset = (y * 256 + x) * 3;
         reference_pixels[offset..offset + 3].copy_from_slice(&[32, 36, 44]);
         if x < 128
         {
            candidate_pixels[offset..offset + 3].copy_from_slice(&[32, 36, 44]);
         }
      }
   }
   let reference = rgb_png(256, 256, &reference_pixels);
   let candidate = rgb_png(256, 256, &candidate_pixels);
   let report = compare_calibrated_static_pngs(&reference, &candidate, &layout(128, 128, "[]"), 2, CalibratedStaticVisualThresholds::default()).expect("compare half-sized visual");

   assert!(report.stable_interior.differing_pixel_ratio < report.thresholds.maximum_stable_interior_differing_pixel_ratio);
   assert!(report.voxels.maximum_mean_absolute_channel_delta > report.thresholds.maximum_voxel_mean_absolute_channel_delta);
   assert!(!report.accepted);
}
