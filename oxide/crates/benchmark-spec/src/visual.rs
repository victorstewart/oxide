use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::io::Cursor;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NORMALIZED_PNG_VISUAL_ALGORITHM: &str = "normalized-srgb8-ssim8-ciede2000-v1";
pub const EXACT_STATIC_VISUAL_ALGORITHM: &str = "normalized-srgb8-exact-static-v1";
pub const CALIBRATED_STATIC_VISUAL_ALGORITHM: &str = "normalized-srgb8-semantic-region-ssim8-voxel-average16-v5";
const SSIM_WINDOW_SIZE: usize = 8;
const SSIM_C1: f64 = 6.5025;
const SSIM_C2: f64 = 58.5225;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct LogicalRect
{
   pub x: f64,
   pub y: f64,
   pub width: f64,
   pub height: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalRect
{
   pub x: u32,
   pub y: u32,
   pub width: u32,
   pub height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct VisualThresholds
{
   pub minimum_non_text_ssim: f64,
   pub maximum_differing_pixel_ratio: f64,
   pub differing_channel_tolerance: u8,
   pub maximum_median_delta_e: f64,
   pub maximum_p99_delta_e: f64,
}

impl Default for VisualThresholds
{
   fn default() -> Self
   {
      Self {
         minimum_non_text_ssim: 0.995,
         maximum_differing_pixel_ratio: 0.005,
         differing_channel_tolerance: 2,
         maximum_median_delta_e: 1.0,
         maximum_p99_delta_e: 3.0,
      }
   }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalibratedStaticVisualThresholds
{
   pub minimum_full_frame_ssim: f64,
   pub maximum_stable_interior_differing_pixel_ratio: f64,
   pub differing_channel_tolerance: u8,
   pub voxel_edge_pixels: u32,
   pub maximum_voxel_mean_absolute_channel_delta: f64,
}

impl Default for CalibratedStaticVisualThresholds
{
   fn default() -> Self
   {
      Self {
         minimum_full_frame_ssim: 0.9795,
         maximum_stable_interior_differing_pixel_ratio: 0.03,
         differing_channel_tolerance: 2,
         voxel_edge_pixels: 16,
         maximum_voxel_mean_absolute_channel_delta: 45.0,
      }
   }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextLineGeometry
{
   pub baseline_y: f64,
   pub ink_bounds: LogicalRect,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextGeometryEvidence
{
   pub reference_lines: Vec<TextLineGeometry>,
   pub candidate_lines: Vec<TextLineGeometry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TextValidationStatus
{
   Validated {
      line_count: usize,
      maximum_baseline_delta_points: f64,
      maximum_ink_bounds_edge_delta_points: f64,
   },
   Pending {
      reason: String,
   },
   Rejected {
      reason: String,
      reference_line_count: usize,
      candidate_line_count: usize,
      maximum_baseline_delta_points: Option<f64>,
      maximum_ink_bounds_edge_delta_points: Option<f64>,
   },
}

impl TextValidationStatus
{
   pub fn is_validated(&self) -> bool
   {
      matches!(self, Self::Validated { .. })
   }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct NonTextVisualMetrics
{
   pub windowed_ssim: f64,
   pub differing_pixel_ratio: f64,
   pub median_delta_e: f64,
   pub p99_delta_e: f64,
   pub compared_pixel_count: u64,
   pub differing_pixel_count: u64,
   pub ssim_window_count: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VisualParityReport
{
   pub algorithm: String,
   pub width: u32,
   pub height: u32,
   pub canonical_scale: u32,
   pub text_masks: Vec<PhysicalRect>,
   pub masked_pixel_count: u64,
   pub non_text: NonTextVisualMetrics,
   pub thresholds: VisualThresholds,
   pub non_text_accepted: bool,
   pub text_validation: TextValidationStatus,
   pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExactStaticVisualParityReport
{
   pub algorithm: String,
   pub oxide_png_sha256: String,
   pub uikit_png_sha256: String,
   pub layout_json_sha256: String,
   pub width: u32,
   pub height: u32,
   pub canonical_scale: u32,
   pub compared_pixel_count: u64,
   pub differing_pixel_count: u64,
   pub differing_channel_count: u64,
   pub maximum_channel_delta: u8,
   pub differing_bounds: Option<PhysicalRect>,
   pub channel_delta_histogram: BTreeMap<u8, u64>,
   pub accepted: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct VoxelVisualMetrics
{
   pub voxel_count: u64,
   pub maximum_mean_absolute_channel_delta: f64,
   pub maximum_delta_voxel: PhysicalRect,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct RasterVisualMetrics
{
   pub windowed_ssim: f64,
   pub differing_pixel_ratio: f64,
   pub compared_pixel_count: u64,
   pub differing_pixel_count: u64,
   pub ssim_window_count: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalibratedStaticVisualParityReport
{
   pub algorithm: String,
   pub width: u32,
   pub height: u32,
   pub canonical_scale: u32,
   pub text_regions: Vec<PhysicalRect>,
   pub text_region_pixel_count: u64,
   pub full_frame: RasterVisualMetrics,
   pub stable_interior: RasterVisualMetrics,
   pub voxels: VoxelVisualMetrics,
   pub thresholds: CalibratedStaticVisualThresholds,
   pub exact_diagnostic: ExactStaticVisualParityReport,
   pub accepted: bool,
}

#[derive(Deserialize)]
struct LayoutContract
{
   #[serde(alias = "coordinateSpace")]
   coordinate_space: String,
   root: LogicalRect,
   #[serde(default)]
   text_masks: Vec<[f64; 4]>,
   #[serde(default)]
   text_comparison: Option<TextComparisonContract>,
   #[serde(default)]
   nodes: Vec<GeometryTextNode>,
}

#[derive(Deserialize)]
struct GeometryTextNode
{
   #[serde(default, alias = "textLineBounds")]
   text_line_bounds: Vec<LogicalRect>,
}

#[derive(Deserialize)]
struct TextComparisonContract
{
   #[serde(default)]
   pixel_mask: bool,
   #[serde(default = "default_text_geometry_tolerance")]
   baseline_tolerance: f64,
   #[serde(default = "default_text_geometry_tolerance")]
   ink_bounds_tolerance: f64,
}

#[derive(Clone)]
struct NormalizedImage
{
   width: u32,
   height: u32,
   rgb: Vec<u8>,
}

pub fn reduce_normalized_png_visual_parity(reference_png: &[u8], candidate_png: &[u8], layout_json: &[u8], canonical_scale: u32, text_geometry: Option<&TextGeometryEvidence>, thresholds: VisualThresholds) -> Result<VisualParityReport>
{
   validate_thresholds(thresholds)?;
   ensure!(canonical_scale > 0, "canonical visual scale must be greater than zero");
   let layout = serde_json::from_slice::<LayoutContract>(layout_json).context("parsing visual layout JSON")?;
   validate_logical_rect(layout.root, "layout root")?;
   ensure!(layout.coordinate_space == "logical-points", "visual layout coordinate space must be logical-points");
   ensure!(layout.root.x == 0.0 && layout.root.y == 0.0, "visual layout root must start at logical origin");

   let reference = decode_normalized_srgb8(reference_png, "reference")?;
   let candidate = decode_normalized_srgb8(candidate_png, "candidate")?;
   ensure!(reference.width == candidate.width && reference.height == candidate.height, "visual PNG dimensions differ: reference is {}x{}, candidate is {}x{}", reference.width, reference.height, candidate.width, candidate.height);
   let expected_width = exact_scaled_dimension(layout.root.width, canonical_scale, "layout root width")?;
   let expected_height = exact_scaled_dimension(layout.root.height, canonical_scale, "layout root height")?;
   ensure!(reference.width == expected_width && reference.height == expected_height, "visual PNG dimensions are {}x{}, expected exact canonical dimensions {}x{}", reference.width, reference.height, expected_width, expected_height);

   let text_masks = rasterize_text_masks(&layout.text_masks, canonical_scale, reference.width, reference.height)?;
   let mask = build_mask(reference.width, reference.height, &text_masks)?;
   let masked_pixel_count = mask.iter().filter(|masked| **masked).count() as u64;
   let non_text = compare_non_text(&reference, &candidate, &mask, thresholds.differing_channel_tolerance)?;
   let non_text_accepted = non_text.windowed_ssim >= thresholds.minimum_non_text_ssim
      && non_text.differing_pixel_ratio <= thresholds.maximum_differing_pixel_ratio
      && non_text.median_delta_e <= thresholds.maximum_median_delta_e
      && non_text.p99_delta_e <= thresholds.maximum_p99_delta_e;
   let text_validation = validate_text_geometry(text_geometry, &layout, !text_masks.is_empty())?;
   let accepted = non_text_accepted && text_validation.is_validated();

   Ok(VisualParityReport {
      algorithm: String::from(NORMALIZED_PNG_VISUAL_ALGORITHM),
      width: reference.width,
      height: reference.height,
      canonical_scale,
      text_masks,
      masked_pixel_count,
      non_text,
      thresholds,
      non_text_accepted,
      text_validation,
      accepted,
   })
}

pub fn compare_exact_static_pngs(oxide_png: &[u8], uikit_png: &[u8], layout_json: &[u8], canonical_scale: u32) -> Result<ExactStaticVisualParityReport>
{
   ensure!(canonical_scale > 0, "canonical visual scale must be greater than zero");
   let layout = serde_json::from_slice::<LayoutContract>(layout_json).context("parsing visual layout JSON")?;
   validate_logical_rect(layout.root, "layout root")?;
   ensure!(layout.coordinate_space == "logical-points", "visual layout coordinate space must be logical-points");
   ensure!(layout.root.x == 0.0 && layout.root.y == 0.0, "visual layout root must start at logical origin");

   let oxide = decode_normalized_srgb8(oxide_png, "Oxide")?;
   let uikit = decode_normalized_srgb8(uikit_png, "UIKit")?;
   ensure!(oxide.width == uikit.width && oxide.height == uikit.height, "static comparison PNG dimensions differ: Oxide is {}x{}, UIKit is {}x{}", oxide.width, oxide.height, uikit.width, uikit.height);
   let expected_width = exact_scaled_dimension(layout.root.width, canonical_scale, "layout root width")?;
   let expected_height = exact_scaled_dimension(layout.root.height, canonical_scale, "layout root height")?;
   ensure!(oxide.width == expected_width && oxide.height == expected_height, "static comparison PNG dimensions are {}x{}, expected exact canonical dimensions {}x{}", oxide.width, oxide.height, expected_width, expected_height);

   let mut differing_pixel_count = 0u64;
   let mut differing_channel_count = 0u64;
   let mut maximum_channel_delta = 0u8;
   let mut minimum_x = oxide.width;
   let mut minimum_y = oxide.height;
   let mut maximum_x = 0u32;
   let mut maximum_y = 0u32;
   let mut channel_delta_counts = [0u64; 256];
   for (pixel_index, (oxide_pixel, uikit_pixel)) in oxide.rgb.chunks_exact(3).zip(uikit.rgb.chunks_exact(3)).enumerate()
   {
      let mut pixel_differs = false;
      for (oxide_channel, uikit_channel) in oxide_pixel.iter().zip(uikit_pixel)
      {
         let delta = oxide_channel.abs_diff(*uikit_channel);
         if delta != 0
         {
            pixel_differs = true;
            differing_channel_count += 1;
            maximum_channel_delta = maximum_channel_delta.max(delta);
            channel_delta_counts[usize::from(delta)] += 1;
         }
      }
      if pixel_differs
      {
         let pixel_index = pixel_index as u32;
         let x = pixel_index % oxide.width;
         let y = pixel_index / oxide.width;
         minimum_x = minimum_x.min(x);
         minimum_y = minimum_y.min(y);
         maximum_x = maximum_x.max(x);
         maximum_y = maximum_y.max(y);
      }
      differing_pixel_count += u64::from(pixel_differs);
   }
   let compared_pixel_count = u64::from(oxide.width) * u64::from(oxide.height);
   let differing_bounds = if differing_pixel_count == 0
   {
      None
   }
   else
   {
      Some(PhysicalRect {
         x: minimum_x,
         y: minimum_y,
         width: maximum_x - minimum_x + 1,
         height: maximum_y - minimum_y + 1,
      })
   };
   let channel_delta_histogram = (1..=u8::MAX).filter_map(|delta|
   {
      let count = channel_delta_counts[usize::from(delta)];
      (count != 0).then_some((delta, count))
   }).collect::<BTreeMap<_, _>>();
   Ok(ExactStaticVisualParityReport {
      algorithm: String::from(EXACT_STATIC_VISUAL_ALGORITHM),
      oxide_png_sha256: format!("{:x}", Sha256::digest(oxide_png)),
      uikit_png_sha256: format!("{:x}", Sha256::digest(uikit_png)),
      layout_json_sha256: format!("{:x}", Sha256::digest(layout_json)),
      width: oxide.width,
      height: oxide.height,
      canonical_scale,
      compared_pixel_count,
      differing_pixel_count,
      differing_channel_count,
      maximum_channel_delta,
      differing_bounds,
      channel_delta_histogram,
      accepted: differing_pixel_count == 0,
   })
}

pub fn compare_calibrated_static_pngs(oxide_png: &[u8], native_png: &[u8], layout_json: &[u8], canonical_scale: u32, thresholds: CalibratedStaticVisualThresholds) -> Result<CalibratedStaticVisualParityReport>
{
   validate_calibrated_static_thresholds(thresholds)?;
   let exact_diagnostic = compare_exact_static_pngs(oxide_png, native_png, layout_json, canonical_scale)?;
   let oxide = decode_normalized_srgb8(oxide_png, "Oxide")?;
   let native = decode_normalized_srgb8(native_png, "native")?;
   let geometry = serde_json::from_slice::<LayoutContract>(layout_json).context("parsing calibrated visual geometry")?;
   let text_padding = 1.0 / f64::from(canonical_scale);
   let logical_text_regions = geometry.nodes.iter().flat_map(|node| &node.text_line_bounds)
      .filter_map(|line|
      {
         padded_clipped_logical_region(*line, geometry.root, text_padding)
      })
      .collect::<Vec<_>>();
   let text_regions = rasterize_text_masks(&logical_text_regions, canonical_scale, oxide.width, oxide.height)?;
   let stable_interior_mask = build_mask(oxide.width, oxide.height, &text_regions)?;
   let text_region_pixel_count = stable_interior_mask.iter().filter(|masked| **masked).count() as u64;
   ensure!(text_region_pixel_count < stable_interior_mask.len() as u64, "calibrated visual text regions cover the entire image");
   let full_frame_mask = vec![false; stable_interior_mask.len()];
   let full_frame = compare_raster(&oxide, &native, &full_frame_mask, thresholds.differing_channel_tolerance)?;
   let stable_interior = compare_raster(&oxide, &native, &stable_interior_mask, thresholds.differing_channel_tolerance)?;
   let voxels = compare_voxels(&oxide, &native, &full_frame_mask, thresholds.voxel_edge_pixels)?;
   let accepted = full_frame.windowed_ssim >= thresholds.minimum_full_frame_ssim
      && stable_interior.differing_pixel_ratio <= thresholds.maximum_stable_interior_differing_pixel_ratio
      && voxels.maximum_mean_absolute_channel_delta <= thresholds.maximum_voxel_mean_absolute_channel_delta;
   Ok(CalibratedStaticVisualParityReport {
      algorithm: String::from(CALIBRATED_STATIC_VISUAL_ALGORITHM),
      width: oxide.width,
      height: oxide.height,
      canonical_scale,
      text_regions,
      text_region_pixel_count,
      full_frame,
      stable_interior,
      voxels,
      thresholds,
      exact_diagnostic,
      accepted,
   })
}

fn padded_clipped_logical_region(rect: LogicalRect, root: LogicalRect, padding: f64) -> Option<[f64; 4]>
{
   let left = (rect.x - padding).max(root.x);
   let top = (rect.y - padding).max(root.y);
   let right = (rect.x + rect.width + padding).min(root.x + root.width);
   let bottom = (rect.y + rect.height + padding).min(root.y + root.height);
   (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

fn default_text_geometry_tolerance() -> f64
{
   0.5
}

fn validate_thresholds(thresholds: VisualThresholds) -> Result<()>
{
   ensure!(thresholds.minimum_non_text_ssim.is_finite() && (0.0..=1.0).contains(&thresholds.minimum_non_text_ssim), "minimum non-text SSIM must be finite and within [0, 1]");
   ensure!(thresholds.maximum_differing_pixel_ratio.is_finite() && (0.0..=1.0).contains(&thresholds.maximum_differing_pixel_ratio), "maximum differing-pixel ratio must be finite and within [0, 1]");
   ensure!(thresholds.maximum_median_delta_e.is_finite() && thresholds.maximum_median_delta_e >= 0.0, "maximum median Delta-E must be finite and nonnegative");
   ensure!(thresholds.maximum_p99_delta_e.is_finite() && thresholds.maximum_p99_delta_e >= 0.0, "maximum p99 Delta-E must be finite and nonnegative");
   Ok(())
}

fn validate_calibrated_static_thresholds(thresholds: CalibratedStaticVisualThresholds) -> Result<()>
{
   ensure!(thresholds.minimum_full_frame_ssim.is_finite() && (0.0..=1.0).contains(&thresholds.minimum_full_frame_ssim), "minimum full-frame SSIM must be finite and within [0, 1]");
   ensure!(thresholds.maximum_stable_interior_differing_pixel_ratio.is_finite() && (0.0..=1.0).contains(&thresholds.maximum_stable_interior_differing_pixel_ratio), "maximum stable-interior differing-pixel ratio must be finite and within [0, 1]");
   ensure!(thresholds.voxel_edge_pixels > 0, "visual voxel edge must be greater than zero");
   ensure!(thresholds.maximum_voxel_mean_absolute_channel_delta.is_finite() && thresholds.maximum_voxel_mean_absolute_channel_delta >= 0.0, "maximum voxel mean absolute channel delta must be finite and nonnegative");
   Ok(())
}

fn decode_normalized_srgb8(bytes: &[u8], label: &str) -> Result<NormalizedImage>
{
   let mut decoder = png::Decoder::new(Cursor::new(bytes));
   decoder.set_transformations(png::Transformations::normalize_to_color8());
   let mut reader = decoder.read_info().with_context(|| format!("decoding {label} PNG header"))?;
   ensure!(reader.info().srgb.is_some(), "{label} PNG does not declare the sRGB color space; exact visual normalization fails closed on ambiguous color encoding");
   let mut decoded = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut decoded).with_context(|| format!("decoding {label} PNG pixels"))?;
   let bytes = &decoded[..info.buffer_size()];
   let pixel_count = (info.width as usize).checked_mul(info.height as usize).context("visual PNG dimensions overflow address space")?;
   let rgb_capacity = pixel_count.checked_mul(3).context("visual RGB buffer size overflows address space")?;
   let mut rgb = Vec::with_capacity(rgb_capacity);
   match info.color_type
   {
      png::ColorType::Rgb => rgb.extend_from_slice(bytes),
      png::ColorType::Rgba =>
      {
         for pixel in bytes.chunks_exact(4)
         {
            ensure!(pixel[3] == 255, "{label} PNG contains nonopaque pixels; normalized screenshot inputs must be opaque sRGB");
            rgb.extend_from_slice(&pixel[..3]);
         }
      }
      png::ColorType::Grayscale =>
      {
         for value in bytes
         {
            rgb.extend_from_slice(&[*value, *value, *value]);
         }
      }
      png::ColorType::GrayscaleAlpha =>
      {
         for pixel in bytes.chunks_exact(2)
         {
            ensure!(pixel[1] == 255, "{label} PNG contains nonopaque pixels; normalized screenshot inputs must be opaque sRGB");
            rgb.extend_from_slice(&[pixel[0], pixel[0], pixel[0]]);
         }
      }
      png::ColorType::Indexed => anyhow::bail!("{label} PNG remained indexed after normalization"),
   }
   ensure!(rgb.len() == rgb_capacity, "{label} PNG decoded byte length differs from its dimensions");
   Ok(NormalizedImage { width: info.width, height: info.height, rgb })
}

fn exact_scaled_dimension(points: f64, scale: u32, label: &str) -> Result<u32>
{
   ensure!(points.is_finite() && points > 0.0, "{label} must be finite and positive");
   let pixels = points * scale as f64;
   ensure!(pixels <= u32::MAX as f64 && pixels.fract() == 0.0, "{label} does not map to an exact physical-pixel dimension at canonical scale {scale}");
   Ok(pixels as u32)
}

fn validate_logical_rect(rect: LogicalRect, label: &str) -> Result<()>
{
   ensure!(rect.x.is_finite() && rect.y.is_finite() && rect.width.is_finite() && rect.height.is_finite(), "{label} contains a non-finite coordinate");
   ensure!(rect.x >= 0.0 && rect.y >= 0.0 && rect.width > 0.0 && rect.height > 0.0, "{label} must be nonnegative with positive dimensions");
   Ok(())
}

fn rasterize_text_masks(masks: &[[f64; 4]], scale: u32, width: u32, height: u32) -> Result<Vec<PhysicalRect>>
{
   let mut physical = Vec::with_capacity(masks.len());
   for (index, mask) in masks.iter().enumerate()
   {
      let logical = LogicalRect { x: mask[0], y: mask[1], width: mask[2], height: mask[3] };
      validate_logical_rect(logical, &format!("text mask {index}"))?;
      let left = (logical.x * scale as f64).floor();
      let top = (logical.y * scale as f64).floor();
      let right = ((logical.x + logical.width) * scale as f64).ceil();
      let bottom = ((logical.y + logical.height) * scale as f64).ceil();
      ensure!(right <= width as f64 && bottom <= height as f64, "text mask {index} extends beyond canonical PNG dimensions");
      let x = left as u32;
      let y = top as u32;
      let right = right as u32;
      let bottom = bottom as u32;
      ensure!(right > x && bottom > y, "text mask {index} rasterizes to an empty rectangle");
      physical.push(PhysicalRect { x, y, width: right - x, height: bottom - y });
   }
   Ok(physical)
}

fn build_mask(width: u32, height: u32, masks: &[PhysicalRect]) -> Result<Vec<bool>>
{
   let pixel_count = (width as usize).checked_mul(height as usize).context("visual mask dimensions overflow address space")?;
   let mut result = vec![false; pixel_count];
   let stride = width as usize;
   for mask in masks
   {
      let right = mask.x + mask.width;
      let bottom = mask.y + mask.height;
      for y in mask.y..bottom
      {
         let row = y as usize * stride;
         for x in mask.x..right
         {
            result[row + x as usize] = true;
         }
      }
   }
   Ok(result)
}

fn compare_non_text(reference: &NormalizedImage, candidate: &NormalizedImage, mask: &[bool], tolerance: u8) -> Result<NonTextVisualMetrics>
{
   let width = reference.width as usize;
   let height = reference.height as usize;
   let mut compared_pixel_count = 0u64;
   let mut differing_pixel_count = 0u64;
   let mut delta_e = Vec::with_capacity(mask.len() - mask.iter().filter(|masked| **masked).count());
   for (index, masked) in mask.iter().enumerate()
   {
      if *masked
      {
         continue;
      }
      let offset = index * 3;
      let reference_rgb = [reference.rgb[offset], reference.rgb[offset + 1], reference.rgb[offset + 2]];
      let candidate_rgb = [candidate.rgb[offset], candidate.rgb[offset + 1], candidate.rgb[offset + 2]];
      compared_pixel_count += 1;
      if reference_rgb.iter().zip(candidate_rgb.iter()).any(|(a, b)| a.abs_diff(*b) > tolerance)
      {
         differing_pixel_count += 1;
      }
      let pixel_delta_e = if reference_rgb == candidate_rgb
      {
         0.0
      }
      else
      {
         ciede2000(srgb_to_lab(reference_rgb), srgb_to_lab(candidate_rgb))
      };
      delta_e.push(pixel_delta_e);
   }
   ensure!(compared_pixel_count > 0, "text masks exclude every visual pixel");
   let (windowed_ssim, ssim_window_count) = windowed_ssim(&reference.rgb, &candidate.rgb, mask, width, height)?;
   let median_delta_e = median(&mut delta_e);
   let p99_delta_e = p99_nearest_rank(&mut delta_e);
   Ok(NonTextVisualMetrics {
      windowed_ssim,
      differing_pixel_ratio: differing_pixel_count as f64 / compared_pixel_count as f64,
      median_delta_e,
      p99_delta_e,
      compared_pixel_count,
      differing_pixel_count,
      ssim_window_count,
   })
}

fn compare_raster(reference: &NormalizedImage, candidate: &NormalizedImage, mask: &[bool], tolerance: u8) -> Result<RasterVisualMetrics>
{
   let width = reference.width as usize;
   let height = reference.height as usize;
   let mut compared_pixel_count = 0u64;
   let mut differing_pixel_count = 0u64;
   for (index, masked) in mask.iter().enumerate()
   {
      if *masked
      {
         continue;
      }
      let offset = index * 3;
      compared_pixel_count += 1;
      if reference.rgb[offset..offset + 3].iter().zip(&candidate.rgb[offset..offset + 3]).any(|(a, b)| a.abs_diff(*b) > tolerance)
      {
         differing_pixel_count += 1;
      }
   }
   ensure!(compared_pixel_count > 0, "visual regions exclude every raster pixel");
   let (windowed_ssim, ssim_window_count) = windowed_ssim(&reference.rgb, &candidate.rgb, mask, width, height)?;
   Ok(RasterVisualMetrics {
      windowed_ssim,
      differing_pixel_ratio: differing_pixel_count as f64 / compared_pixel_count as f64,
      compared_pixel_count,
      differing_pixel_count,
      ssim_window_count,
   })
}

fn compare_voxels(reference: &NormalizedImage, candidate: &NormalizedImage, mask: &[bool], edge: u32) -> Result<VoxelVisualMetrics>
{
   ensure!(reference.width == candidate.width && reference.height == candidate.height, "voxel comparison dimensions differ");
   ensure!(mask.len() == reference.width as usize * reference.height as usize, "voxel comparison mask dimensions differ");
   let mut voxel_count = 0u64;
   let mut maximum_mean_absolute_channel_delta = 0.0f64;
   let mut maximum_delta_voxel = PhysicalRect {x: 0, y: 0, width: edge.min(reference.width), height: edge.min(reference.height)};
   for top in (0..reference.height).step_by(edge as usize)
   {
      for left in (0..reference.width).step_by(edge as usize)
      {
         let bottom = (top + edge).min(reference.height);
         let right = (left + edge).min(reference.width);
         let mut reference_channel_sums = [0u64; 3];
         let mut candidate_channel_sums = [0u64; 3];
         let mut pixel_count = 0u64;
         for y in top..bottom
         {
            for x in left..right
            {
               let pixel_index = y as usize * reference.width as usize + x as usize;
               if mask[pixel_index]
               {
                  continue;
               }
               let offset = (y as usize * reference.width as usize + x as usize) * 3;
               for channel in 0..3
               {
                  reference_channel_sums[channel] += u64::from(reference.rgb[offset + channel]);
                  candidate_channel_sums[channel] += u64::from(candidate.rgb[offset + channel]);
               }
               pixel_count += 1;
            }
         }
         if pixel_count == 0
         {
            continue;
         }
         let absolute_channel_mean_delta = reference_channel_sums.iter()
            .zip(candidate_channel_sums)
            .map(|(reference, candidate)| reference.abs_diff(candidate))
            .sum::<u64>();
         let mean = absolute_channel_mean_delta as f64 / (pixel_count * 3) as f64;
         if mean > maximum_mean_absolute_channel_delta
         {
            maximum_mean_absolute_channel_delta = mean;
            maximum_delta_voxel = PhysicalRect {x: left, y: top, width: right - left, height: bottom - top};
         }
         voxel_count += 1;
      }
   }
   ensure!(voxel_count > 0, "visual image contains no voxels");
   Ok(VoxelVisualMetrics {voxel_count, maximum_mean_absolute_channel_delta, maximum_delta_voxel})
}

fn windowed_ssim(reference: &[u8], candidate: &[u8], mask: &[bool], width: usize, height: usize) -> Result<(f64, u64)>
{
   let mut weighted_ssim = 0.0;
   let mut total_weight = 0u64;
   let mut window_count = 0u64;
   for top in (0..height).step_by(SSIM_WINDOW_SIZE)
   {
      for left in (0..width).step_by(SSIM_WINDOW_SIZE)
      {
         let bottom = (top + SSIM_WINDOW_SIZE).min(height);
         let right = (left + SSIM_WINDOW_SIZE).min(width);
         let mut count = 0u64;
         let mut sum_reference = 0.0;
         let mut sum_candidate = 0.0;
         let mut sum_reference_sq = 0.0;
         let mut sum_candidate_sq = 0.0;
         let mut sum_product = 0.0;
         for y in top..bottom
         {
            for x in left..right
            {
               let pixel = y * width + x;
               if mask[pixel]
               {
                  continue;
               }
               let offset = pixel * 3;
               let a = srgb_luma(&reference[offset..offset + 3]);
               let b = srgb_luma(&candidate[offset..offset + 3]);
               count += 1;
               sum_reference += a;
               sum_candidate += b;
               sum_reference_sq += a * a;
               sum_candidate_sq += b * b;
               sum_product += a * b;
            }
         }
         if count < 2
         {
            continue;
         }
         let denominator = count as f64;
         let mean_reference = sum_reference / denominator;
         let mean_candidate = sum_candidate / denominator;
         let variance_reference = (sum_reference_sq / denominator - mean_reference * mean_reference).max(0.0);
         let variance_candidate = (sum_candidate_sq / denominator - mean_candidate * mean_candidate).max(0.0);
         let covariance = sum_product / denominator - mean_reference * mean_candidate;
         let luminance = (2.0 * mean_reference * mean_candidate + SSIM_C1) / (mean_reference * mean_reference + mean_candidate * mean_candidate + SSIM_C1);
         let structure = (2.0 * covariance + SSIM_C2) / (variance_reference + variance_candidate + SSIM_C2);
         weighted_ssim += luminance * structure * denominator;
         total_weight += count;
         window_count += 1;
      }
   }
   ensure!(window_count > 0 && total_weight > 0, "no SSIM window contains at least two non-text pixels");
   Ok((weighted_ssim / total_weight as f64, window_count))
}

fn srgb_luma(rgb: &[u8]) -> f64
{
   0.2126 * rgb[0] as f64 + 0.7152 * rgb[1] as f64 + 0.0722 * rgb[2] as f64
}

fn median(values: &mut [f64]) -> f64
{
   let length = values.len();
   let upper_index = length / 2;
   let (lower_partition, upper, _) = values.select_nth_unstable_by(upper_index, f64::total_cmp);
   if length % 2 == 1
   {
      *upper
   }
   else
   {
      let lower = lower_partition.iter().copied().max_by(f64::total_cmp).unwrap_or(*upper);
      (lower + *upper) * 0.5
   }
}

fn p99_nearest_rank(values: &mut [f64]) -> f64
{
   let rank = values.len() - values.len() / 100;
   let index = rank.saturating_sub(1).min(values.len() - 1);
   let (_, value, _) = values.select_nth_unstable_by(index, f64::total_cmp);
   *value
}

fn validate_text_geometry(evidence: Option<&TextGeometryEvidence>, layout: &LayoutContract, has_masks: bool) -> Result<TextValidationStatus>
{
   let Some(evidence) = evidence else
   {
      return Ok(TextValidationStatus::Pending { reason: String::from("text line-count, baseline, and ink-bound evidence is unavailable") });
   };
   for (index, line) in evidence.reference_lines.iter().enumerate()
   {
      validate_text_line(*line, &format!("reference text line {index}"))?;
   }
   for (index, line) in evidence.candidate_lines.iter().enumerate()
   {
      validate_text_line(*line, &format!("candidate text line {index}"))?;
   }
   if evidence.reference_lines.is_empty() && evidence.candidate_lines.is_empty()
   {
      return Ok(TextValidationStatus::Pending { reason: String::from("text geometry evidence contains no lines") });
   }
   if !has_masks
   {
      let reason = if layout.text_comparison.as_ref().is_some_and(|contract| contract.pixel_mask)
      {
         "layout requests text masking but provides no logical-point text_masks"
      }
      else
      {
         "layout provides no logical-point text_masks"
      };
      return Ok(TextValidationStatus::Pending { reason: String::from(reason) });
   }
   if evidence.reference_lines.len() != evidence.candidate_lines.len()
   {
      return Ok(TextValidationStatus::Rejected {
         reason: String::from("text line counts differ"),
         reference_line_count: evidence.reference_lines.len(),
         candidate_line_count: evidence.candidate_lines.len(),
         maximum_baseline_delta_points: None,
         maximum_ink_bounds_edge_delta_points: None,
      });
   }
   let comparison = layout.text_comparison.as_ref();
   let baseline_tolerance = comparison.map_or(default_text_geometry_tolerance(), |contract| contract.baseline_tolerance);
   let ink_bounds_tolerance = comparison.map_or(default_text_geometry_tolerance(), |contract| contract.ink_bounds_tolerance);
   ensure!(baseline_tolerance.is_finite() && baseline_tolerance >= 0.0, "text baseline tolerance must be finite and nonnegative");
   ensure!(ink_bounds_tolerance.is_finite() && ink_bounds_tolerance >= 0.0, "text ink-bounds tolerance must be finite and nonnegative");
   let mut maximum_baseline_delta = 0.0f64;
   let mut maximum_ink_delta = 0.0f64;
   for (reference, candidate) in evidence.reference_lines.iter().zip(evidence.candidate_lines.iter())
   {
      maximum_baseline_delta = maximum_baseline_delta.max((reference.baseline_y - candidate.baseline_y).abs());
      maximum_ink_delta = maximum_ink_delta.max(maximum_rect_edge_delta(reference.ink_bounds, candidate.ink_bounds));
   }
   if maximum_baseline_delta > baseline_tolerance || maximum_ink_delta > ink_bounds_tolerance
   {
      return Ok(TextValidationStatus::Rejected {
         reason: String::from("text baseline or ink bounds exceed the layout tolerance"),
         reference_line_count: evidence.reference_lines.len(),
         candidate_line_count: evidence.candidate_lines.len(),
         maximum_baseline_delta_points: Some(maximum_baseline_delta),
         maximum_ink_bounds_edge_delta_points: Some(maximum_ink_delta),
      });
   }
   Ok(TextValidationStatus::Validated {
      line_count: evidence.reference_lines.len(),
      maximum_baseline_delta_points: maximum_baseline_delta,
      maximum_ink_bounds_edge_delta_points: maximum_ink_delta,
   })
}

fn validate_text_line(line: TextLineGeometry, label: &str) -> Result<()>
{
   ensure!(line.baseline_y.is_finite() && line.baseline_y >= 0.0, "{label} has an invalid baseline");
   validate_logical_rect(line.ink_bounds, &format!("{label} ink bounds"))
}

fn maximum_rect_edge_delta(reference: LogicalRect, candidate: LogicalRect) -> f64
{
   let reference_right = reference.x + reference.width;
   let reference_bottom = reference.y + reference.height;
   let candidate_right = candidate.x + candidate.width;
   let candidate_bottom = candidate.y + candidate.height;
   (reference.x - candidate.x).abs()
      .max((reference.y - candidate.y).abs())
      .max((reference_right - candidate_right).abs())
      .max((reference_bottom - candidate_bottom).abs())
}

fn srgb_to_lab(rgb: [u8; 3]) -> [f64; 3]
{
   let r = srgb_channel_to_linear(rgb[0]);
   let g = srgb_channel_to_linear(rgb[1]);
   let b = srgb_channel_to_linear(rgb[2]);
   let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
   let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
   let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
   let fx = lab_axis(x);
   let fy = lab_axis(y);
   let fz = lab_axis(z);
   [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn srgb_channel_to_linear(value: u8) -> f64
{
   let value = value as f64 / 255.0;
   if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

fn lab_axis(value: f64) -> f64
{
   const DELTA: f64 = 6.0 / 29.0;
   if value > DELTA * DELTA * DELTA { value.cbrt() } else { value / (3.0 * DELTA * DELTA) + 4.0 / 29.0 }
}

fn ciede2000(reference: [f64; 3], candidate: [f64; 3]) -> f64
{
   let [l1, a1, b1] = reference;
   let [l2, a2, b2] = candidate;
   let c1 = a1.hypot(b1);
   let c2 = a2.hypot(b2);
   let mean_c = (c1 + c2) * 0.5;
   let mean_c7 = mean_c.powi(7);
   let g = 0.5 * (1.0 - (mean_c7 / (mean_c7 + 25.0f64.powi(7))).sqrt());
   let a1_prime = (1.0 + g) * a1;
   let a2_prime = (1.0 + g) * a2;
   let c1_prime = a1_prime.hypot(b1);
   let c2_prime = a2_prime.hypot(b2);
   let h1_prime = hue_degrees(b1, a1_prime);
   let h2_prime = hue_degrees(b2, a2_prime);
   let delta_l_prime = l2 - l1;
   let delta_c_prime = c2_prime - c1_prime;
   let delta_h_degrees = if c1_prime * c2_prime == 0.0
   {
      0.0
   }
   else if (h2_prime - h1_prime).abs() <= 180.0
   {
      h2_prime - h1_prime
   }
   else if h2_prime <= h1_prime
   {
      h2_prime - h1_prime + 360.0
   }
   else
   {
      h2_prime - h1_prime - 360.0
   };
   let delta_h_prime = 2.0 * (c1_prime * c2_prime).sqrt() * degrees_to_radians(delta_h_degrees * 0.5).sin();
   let mean_l_prime = (l1 + l2) * 0.5;
   let mean_c_prime = (c1_prime + c2_prime) * 0.5;
   let mean_h_prime = if c1_prime * c2_prime == 0.0
   {
      h1_prime + h2_prime
   }
   else if (h1_prime - h2_prime).abs() <= 180.0
   {
      (h1_prime + h2_prime) * 0.5
   }
   else if h1_prime + h2_prime < 360.0
   {
      (h1_prime + h2_prime + 360.0) * 0.5
   }
   else
   {
      (h1_prime + h2_prime - 360.0) * 0.5
   };
   let t = 1.0
      - 0.17 * degrees_to_radians(mean_h_prime - 30.0).cos()
      + 0.24 * degrees_to_radians(2.0 * mean_h_prime).cos()
      + 0.32 * degrees_to_radians(3.0 * mean_h_prime + 6.0).cos()
      - 0.20 * degrees_to_radians(4.0 * mean_h_prime - 63.0).cos();
   let delta_theta = 30.0 * (-((mean_h_prime - 275.0) / 25.0).powi(2)).exp();
   let mean_c_prime7 = mean_c_prime.powi(7);
   let r_c = 2.0 * (mean_c_prime7 / (mean_c_prime7 + 25.0f64.powi(7))).sqrt();
   let l_offset = mean_l_prime - 50.0;
   let s_l = 1.0 + 0.015 * l_offset * l_offset / (20.0 + l_offset * l_offset).sqrt();
   let s_c = 1.0 + 0.045 * mean_c_prime;
   let s_h = 1.0 + 0.015 * mean_c_prime * t;
   let r_t = -degrees_to_radians(2.0 * delta_theta).sin() * r_c;
   let l_term = delta_l_prime / s_l;
   let c_term = delta_c_prime / s_c;
   let h_term = delta_h_prime / s_h;
   (l_term * l_term + c_term * c_term + h_term * h_term + r_t * c_term * h_term).max(0.0).sqrt()
}

fn hue_degrees(b: f64, a: f64) -> f64
{
   let degrees = b.atan2(a) * 180.0 / PI;
   if degrees < 0.0 { degrees + 360.0 } else { degrees }
}

fn degrees_to_radians(degrees: f64) -> f64
{
   degrees * PI / 180.0
}
