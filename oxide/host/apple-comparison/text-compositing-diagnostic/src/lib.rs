//! Comparison-only CoreText/A8/Metal-quantization title diagnostic.

use std::{
   error::Error,
   fmt,
   fs,
   path::{Path, PathBuf},
   process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod production_probe;

/// Frozen physical width shared by both title probes.
pub const CANVAS_WIDTH: u32 = 1_170;
/// Frozen physical height containing both title ink bounds.
pub const CANVAS_HEIGHT: u32 = 174;
/// Canonical Apple comparison scale.
pub const DEVICE_SCALE: u32 = 3;
/// Required pinned Noto Sans variable-font identity.
pub const FONT_SHA256: &str = "bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d";

const EXPECTED_CASE_IDENTITIES: [(&str, &str, &str, &str, &str); 2] = [
   (
      "navigation",
      "ffcfea7d4c7cd844a7e49b91d8ac1d4e18a83b10e3cdd4d6de65949937de4729",
      "a5be0652c49f70f47ef7a79825f14645dc7cd4ed67d1dc0563fe56df61b9a2d9",
      "a5be0652c49f70f47ef7a79825f14645dc7cd4ed67d1dc0563fe56df61b9a2d9",
      "687c0f9288e176d816bc8869af668e9745dfe98d244e03bf8e06040180a09cb3",
   ),
   (
      "image",
      "d4fc7a761b0c00e7268365402b0c5565d78a2bdbc30f9907f3d561f5ecb7b17c",
      "f1eac519edd2fa4a9f521b03007274fcb849169f8e312b250010f133a65ecc3a",
      "f1eac519edd2fa4a9f521b03007274fcb849169f8e312b250010f133a65ecc3a",
      "0565f6e8a96eea2350639f53058383d1e75aeed8e81a94d94983e59d78af3d51",
   ),
];

const EXPECTED_METAL_IDENTITIES: [(&str, &str, &str, usize, &str, &str, &str); 2] = [
   (
      "navigation",
      "a89948b737e0414cc51d95aedb1fb2653f894933cb9954b7c274a2ff7fbc6dc9",
      "c6c58c0e11544168432d58e9f67753323d7d887f0be4acf950d0776b0be7cddb",
      10,
      "d979e2cd81a80dabb7422ecb600d79071724fecfbc8cd13ed272de32201a308f",
      "687c0f9288e176d816bc8869af668e9745dfe98d244e03bf8e06040180a09cb3",
      "a7b6e99afd0048f0b022594015087ca706217e18b117b60bb188caa0997d8be7",
   ),
   (
      "image",
      "5640c2651dc5b37139ad4f1c4eb2b00812ee5e14a5381f98081b3dafee0fe4e9",
      "534a809a96b115e2a47e9d5c9b7481556fefd71dfb9d5d08ef5d35e73e93421e",
      11,
      "5e72219b1f64c47d9b2d137877f5360312644ddb93400ba947bc27c35d8c6243",
      "0565f6e8a96eea2350639f53058383d1e75aeed8e81a94d94983e59d78af3d51",
      "39d1420fa6302215975a13f206bf87a915707eb7cb1ebffffb17e4249efc639d",
   ),
];

/// One exact v28 title pixel used to validate the compositing attribution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnownSignature
{
   /// Physical x coordinate in the canonical screenshot.
   pub x: u32,
   /// Physical y coordinate in the canonical screenshot.
   pub y: u32,
   /// Glyph owning the sample.
   pub glyph: &'static str,
   /// Native AppKit canonical RGB8 value.
   pub expected_native_rgb: [u8; 3],
   /// Oxide Metal canonical RGB8 value.
   pub expected_oxide_rgb: [u8; 3],
}

/// One immutable title geometry and its known v28 signatures.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TitleCase
{
   /// Stable title identity.
   pub id: &'static str,
   /// Rendered UTF-8 text.
   pub text: &'static str,
   /// Logical frame `[x, y, width, height]` in millionths of a point.
   pub frame_millionths: [i64; 4],
   /// CoreText horizontal alignment.
   pub alignment: &'static str,
   /// Known exact residual coordinates.
   pub signatures: Vec<KnownSignature>,
}

/// Complete request consumed by the isolated Swift helper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiagnosticRequest
{
   /// Request schema identity.
   pub schema_version: u32,
   /// Physical output width.
   pub canvas_width: u32,
   /// Physical output height.
   pub canvas_height: u32,
   /// Canonical device scale.
   pub device_scale: u32,
   /// Point size in millionths.
   pub point_size_millionths: u32,
   /// Opaque sRGB8 background.
   pub background_srgb: [u8; 3],
   /// Opaque sRGB8 text color.
   pub text_srgb: [u8; 3],
   /// Required font hash.
   pub font_sha256: &'static str,
   /// Variable-font axis coordinates in millionths.
   pub variations: Vec<FontVariation>,
   /// Frozen title cases.
   pub cases: Vec<TitleCase>,
}

/// One pinned OpenType variation axis.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FontVariation
{
   /// Four-byte OpenType tag.
   pub tag: &'static str,
   /// Axis coordinate in millionths.
   pub value_millionths: i64,
}

/// Validated report identity returned by a complete diagnostic run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DiagnosticSummary
{
   /// Report schema identity.
   pub schema_version: u32,
   /// Reducer identity.
   pub algorithm: String,
   /// Pinned font identity returned by Swift.
   pub font_sha256: String,
   /// Number of title cases.
   pub case_count: usize,
   /// Number of known signatures.
   pub signature_count: usize,
   /// Signatures whose direct path equals v28 AppKit.
   pub direct_native_match_count: usize,
   /// Signatures whose CPU Metal replay equals v28 Oxide.
   pub metal_oxide_match_count: usize,
   /// Signatures whose A8 mask-fill equals v28 AppKit.
   pub mask_fill_native_match_count: usize,
   /// Persisted report path.
   #[serde(default)]
   pub report_path: PathBuf,
   /// Per-case artifact identities.
   pub cases: Vec<CaseSummary>,
   /// Exact production-atlas Metal stage attribution.
   #[serde(skip)]
   pub metal_probe: Option<MetalProbeSummary>,
}

/// Hashes and signature counts for one title case.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CaseSummary
{
   /// Stable title identity.
   pub id: String,
   /// A8 artifact filename.
   pub a8_path: String,
   /// A8 SHA-256.
   pub a8_sha256: String,
   /// Direct RGB artifact filename.
   pub direct_rgb_path: String,
   /// Direct RGB SHA-256.
   pub direct_rgb_sha256: String,
   /// A8 mask-fill artifact filename.
   pub mask_fill_rgb_path: String,
   /// A8 mask-fill SHA-256.
   pub mask_fill_rgb_sha256: String,
   /// CPU Metal replay artifact filename.
   pub metal_cpu_rgb_path: String,
   /// CPU Metal replay SHA-256.
   pub metal_cpu_rgb_sha256: String,
   /// Number of known signatures in the case.
   pub signature_count: usize,
}

/// Validated offscreen production-Metal attribution report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct MetalProbeSummary
{
   /// Report schema identity.
   pub schema_version: u32,
   /// Metal-stage reducer identity.
   pub algorithm: String,
   /// GPU used by the offscreen probe.
   pub device_name: String,
   /// Number of title cases.
   pub case_count: usize,
   /// Number of frozen residual signatures.
   pub signature_count: usize,
   /// Oxide matches after point sampling and CPU source-over.
   pub point_cpu_oxide_match_count: usize,
   /// Oxide matches after production linear sampling and CPU source-over.
   pub linear_cpu_oxide_match_count: usize,
   /// Oxide matches after the complete production Metal path.
   pub metal_full_oxide_match_count: usize,
   /// Signatures already explained by the scalar A8 replay.
   pub earliest_scalar_a8_replay_count: usize,
   /// Signatures first explained by production atlas rasterization/geometry.
   pub earliest_atlas_raster_and_geometry_count: usize,
   /// Signatures first explained by the linear sampler.
   pub earliest_linear_sampler_count: usize,
   /// Signatures first explained by fixed blending into the sRGB target.
   pub earliest_fixed_blend_srgb_target_count: usize,
   /// Signatures not reproduced by the complete probe.
   pub unexplained_count: usize,
   /// Per-case Metal artifact identities.
   pub cases: Vec<MetalProbeCaseSummary>,
   /// Persisted Metal report path.
   #[serde(default)]
   pub report_path: PathBuf,
}

/// One title's exact production-atlas Metal artifacts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct MetalProbeCaseSummary
{
   /// Stable title identity.
   pub id: String,
   /// Production geometry manifest SHA-256.
   pub manifest_sha256: String,
   /// Production A8 atlas artifact filename.
   pub atlas_path: String,
   /// Production A8 atlas SHA-256.
   pub atlas_sha256: String,
   /// Lowered production glyph instance count.
   pub instance_count: usize,
   /// Point-sampled coverage artifact filename.
   pub point_coverage_path: String,
   /// Point-sampled coverage SHA-256.
   pub point_coverage_sha256: String,
   /// Linear-sampled coverage artifact filename.
   pub linear_coverage_path: String,
   /// Linear-sampled coverage SHA-256.
   pub linear_coverage_sha256: String,
   /// Point-sampled CPU composite artifact filename.
   pub point_cpu_rgb_path: String,
   /// Point-sampled CPU composite SHA-256.
   pub point_cpu_rgb_sha256: String,
   /// Linear-sampled CPU composite artifact filename.
   pub linear_cpu_rgb_path: String,
   /// Linear-sampled CPU composite SHA-256.
   pub linear_cpu_rgb_sha256: String,
   /// Complete fixed-blend Metal artifact filename.
   pub metal_full_rgb_path: String,
   /// Complete fixed-blend Metal SHA-256.
   pub metal_full_rgb_sha256: String,
   /// Full-canvas point-versus-linear coverage difference count.
   pub point_vs_linear_differing_pixel_count: usize,
   /// Full-canvas scalar-versus-point RGB difference count.
   pub scalar_vs_point_differing_pixel_count: usize,
   /// Frozen signature count for the case.
   pub signature_count: usize,
}

/// Fail-closed setup, helper, or report-contract error.
#[derive(Debug)]
pub struct DiagnosticError(String);

impl DiagnosticError
{
   fn new(message: impl Into<String>) -> Self
   {
      Self(message.into())
   }
}

impl fmt::Display for DiagnosticError
{
   fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      formatter.write_str(&self.0)
   }
}

impl Error for DiagnosticError {}

/// Returns the frozen two-title, 35-signature request.
pub fn diagnostic_request() -> DiagnosticRequest
{
   DiagnosticRequest {
      schema_version: 1,
      canvas_width: CANVAS_WIDTH,
      canvas_height: CANVAS_HEIGHT,
      device_scale: DEVICE_SCALE,
      point_size_millionths: 20_000_000,
      background_srgb: [243, 245, 248],
      text_srgb: [32, 36, 44],
      font_sha256: FONT_SHA256,
      variations: vec![
         FontVariation {tag: "wght", value_millionths: 400_000_000},
         FontVariation {tag: "wdth", value_millionths: 100_000_000},
      ],
      cases: vec![navigation_case(), image_case()],
   }
}

/// Returns a lowercase SHA-256 identity.
pub fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

/// Persists the request, invokes the offscreen Swift helper, and validates every artifact identity.
pub fn run(font_path: &Path, output_directory: &Path) -> Result<DiagnosticSummary, DiagnosticError>
{
   let font = fs::read(font_path).map_err(|error| DiagnosticError::new(format!("reading {}: {error}", font_path.display())))?;
   if sha256(&font) != FONT_SHA256
   {
      return Err(DiagnosticError::new("font identity differs from the pinned Noto Sans VF"));
   }
   fs::create_dir_all(output_directory).map_err(|error| DiagnosticError::new(format!("creating {}: {error}", output_directory.display())))?;
   let diagnostic = diagnostic_request();
   production_probe::prepare(&diagnostic, &font, output_directory)
      .map_err(|error| DiagnosticError::new(format!("preparing production Metal probe: {error}")))?;
   let request_path = output_directory.join("request.json");
   let report_path = output_directory.join("report.json");
   let encoded_request = serde_json::to_vec_pretty(&diagnostic).map_err(|error| DiagnosticError::new(format!("encoding request: {error}")))?;
   fs::write(&request_path, encoded_request).map_err(|error| DiagnosticError::new(format!("writing {}: {error}", request_path.display())))?;

   let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("TextCompositingReference.swift");
   let output = Command::new("xcrun")
      .arg("swift")
      .arg(&helper)
      .arg(font_path)
      .arg(&request_path)
      .arg(output_directory)
      .arg(&report_path)
      .output()
      .map_err(|error| DiagnosticError::new(format!("launching CoreText helper: {error}")))?;
   if !output.status.success()
   {
      return Err(DiagnosticError::new(format!("CoreText helper failed with {}: {}", output.status, String::from_utf8_lossy(&output.stderr))));
   }

   let report = fs::read(&report_path).map_err(|error| DiagnosticError::new(format!("reading {}: {error}", report_path.display())))?;
   let mut summary = serde_json::from_slice::<DiagnosticSummary>(&report).map_err(|error| DiagnosticError::new(format!("parsing report: {error}")))?;
   let request = diagnostic;
   if summary.schema_version != 1 || summary.algorithm != "coretext-a8-metal-text-compositing-v1"
   {
      return Err(DiagnosticError::new("diagnostic report identity is unsupported"));
   }
   if summary.font_sha256 != FONT_SHA256 || summary.case_count != request.cases.len()
      || summary.signature_count != request.cases.iter().map(|case| case.signatures.len()).sum::<usize>()
      || summary.cases.len() != request.cases.len()
      || summary.direct_native_match_count != 30
      || summary.mask_fill_native_match_count != 30
      || summary.metal_oxide_match_count != 5
   {
      return Err(DiagnosticError::new("diagnostic report is incomplete or has the wrong font identity"));
   }
   for (case, expected) in summary.cases.iter().zip(EXPECTED_CASE_IDENTITIES)
   {
      if case.id != expected.0 || case.a8_sha256 != expected.1 || case.direct_rgb_sha256 != expected.2
         || case.mask_fill_rgb_sha256 != expected.3 || case.metal_cpu_rgb_sha256 != expected.4
      {
         return Err(DiagnosticError::new(format!("frozen artifact identity differs for {}", case.id)));
      }
      validate_artifact(output_directory, &case.a8_path, &case.a8_sha256, CANVAS_WIDTH as usize * CANVAS_HEIGHT as usize)?;
      let rgb_len = CANVAS_WIDTH as usize * CANVAS_HEIGHT as usize * 3;
      validate_artifact(output_directory, &case.direct_rgb_path, &case.direct_rgb_sha256, rgb_len)?;
      validate_artifact(output_directory, &case.mask_fill_rgb_path, &case.mask_fill_rgb_sha256, rgb_len)?;
      validate_artifact(output_directory, &case.metal_cpu_rgb_path, &case.metal_cpu_rgb_sha256, rgb_len)?;
   }
   let metal_report_path = output_directory.join("metal-report.json");
   let metal_helper = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("MetalTextProbe.swift");
   let metal_output = Command::new("xcrun")
      .arg("swift")
      .arg(&metal_helper)
      .arg(&request_path)
      .arg(output_directory)
      .arg(&metal_report_path)
      .output()
      .map_err(|error| DiagnosticError::new(format!("launching Metal text probe: {error}")))?;
   if !metal_output.status.success()
   {
      return Err(DiagnosticError::new(format!("Metal text probe failed with {}: {}", metal_output.status, String::from_utf8_lossy(&metal_output.stderr))));
   }
   let metal_report = fs::read(&metal_report_path)
      .map_err(|error| DiagnosticError::new(format!("reading {}: {error}", metal_report_path.display())))?;
   let mut metal = serde_json::from_slice::<MetalProbeSummary>(&metal_report)
      .map_err(|error| DiagnosticError::new(format!("parsing Metal report: {error}")))?;
   if metal.schema_version != 1 || metal.algorithm != "production-atlas-metal-stage-attribution-v1"
      || metal.case_count != 2 || metal.signature_count != 35 || metal.cases.len() != 2
      || metal.point_cpu_oxide_match_count != 5 || metal.linear_cpu_oxide_match_count != 5
      || metal.metal_full_oxide_match_count != 35 || metal.earliest_scalar_a8_replay_count != 5
      || metal.earliest_atlas_raster_and_geometry_count != 0 || metal.earliest_linear_sampler_count != 0
      || metal.earliest_fixed_blend_srgb_target_count != 30 || metal.unexplained_count != 0
   {
      return Err(DiagnosticError::new("production Metal attribution differs from the frozen 5/0/0/30 result"));
   }
   let coverage_len = CANVAS_WIDTH as usize * CANVAS_HEIGHT as usize * core::mem::size_of::<f32>();
   let rgb_len = CANVAS_WIDTH as usize * CANVAS_HEIGHT as usize * 3;
   for ((case, core_case), expected) in metal.cases.iter().zip(summary.cases.iter()).zip(EXPECTED_METAL_IDENTITIES)
   {
      if case.id != expected.0 || case.manifest_sha256 != expected.1 || case.atlas_sha256 != expected.2
         || case.instance_count != expected.3 || case.point_coverage_sha256 != expected.4
         || case.linear_coverage_sha256 != expected.4 || case.point_cpu_rgb_sha256 != expected.5
         || case.linear_cpu_rgb_sha256 != expected.5 || case.metal_full_rgb_sha256 != expected.6
         || case.point_cpu_rgb_sha256 != core_case.metal_cpu_rgb_sha256
         || case.point_vs_linear_differing_pixel_count != 0 || case.scalar_vs_point_differing_pixel_count != 0
         || case.signature_count != core_case.signature_count
      {
         return Err(DiagnosticError::new(format!("frozen production Metal identity differs for {}", case.id)));
      }
      validate_hash(output_directory, &format!("{}.production-probe.json", case.id), &case.manifest_sha256)?;
      validate_artifact(output_directory, &case.atlas_path, &case.atlas_sha256, 1_024 * 1_024)?;
      validate_artifact(output_directory, &case.point_coverage_path, &case.point_coverage_sha256, coverage_len)?;
      validate_artifact(output_directory, &case.linear_coverage_path, &case.linear_coverage_sha256, coverage_len)?;
      validate_artifact(output_directory, &case.point_cpu_rgb_path, &case.point_cpu_rgb_sha256, rgb_len)?;
      validate_artifact(output_directory, &case.linear_cpu_rgb_path, &case.linear_cpu_rgb_sha256, rgb_len)?;
      validate_artifact(output_directory, &case.metal_full_rgb_path, &case.metal_full_rgb_sha256, rgb_len)?;
   }
   metal.report_path = metal_report_path;
   summary.metal_probe = Some(metal);
   summary.report_path = report_path;
   Ok(summary)
}

fn validate_hash(root: &Path, relative: &str, expected_sha256: &str) -> Result<(), DiagnosticError>
{
   let path = root.join(relative);
   let bytes = fs::read(&path).map_err(|error| DiagnosticError::new(format!("reading {}: {error}", path.display())))?;
   if sha256(&bytes) != expected_sha256
   {
      return Err(DiagnosticError::new(format!("artifact identity differs for {}", path.display())));
   }
   Ok(())
}

fn validate_artifact(root: &Path, relative: &str, expected_sha256: &str, expected_len: usize) -> Result<(), DiagnosticError>
{
   let path = root.join(relative);
   let bytes = fs::read(&path).map_err(|error| DiagnosticError::new(format!("reading {}: {error}", path.display())))?;
   if bytes.len() != expected_len || sha256(&bytes) != expected_sha256
   {
      return Err(DiagnosticError::new(format!("artifact identity differs for {}", path.display())));
   }
   Ok(())
}

fn signature(x: u32, y: u32, glyph: &'static str, kind: u8) -> KnownSignature
{
   let (expected_native_rgb, expected_oxide_rgb) = match kind
   {
      0 => ([180, 181, 184], [179, 181, 184]),
      1 => ([235, 237, 239], [235, 236, 239]),
      _ => ([231, 233, 235], [231, 233, 236]),
   };
   KnownSignature {x, y, glyph, expected_native_rgb, expected_oxide_rgb}
}

fn navigation_case() -> TitleCase
{
   TitleCase {
      id: "navigation",
      text: "Navigation",
      frame_millionths: [-333_333, 6_000_000, 390_000_000, 52_000_000],
      alignment: "center",
      signatures: vec![
         signature(446, 81, "N", 0), signature(550, 76, "i", 0), signature(571, 119, "g", 0), signature(658, 76, "i", 0),
         signature(458, 110, "N", 1), signature(478, 111, "a", 1), signature(503, 90, "a", 1), signature(562, 90, "g", 1),
         signature(581, 109, "g", 1), signature(595, 111, "a", 1), signature(620, 90, "a", 1), signature(672, 117, "o", 1),
         signature(711, 89, "n", 1), signature(722, 91, "n", 1), signature(460, 113, "N", 2), signature(646, 114, "t", 2),
      ],
   }
}

fn image_case() -> TitleCase
{
   TitleCase {
      id: "image",
      text: "Decode & Zoom",
      frame_millionths: [18_000_000, 8_000_000, 204_000_000, 36_000_000],
      alignment: "left",
      signatures: vec![
         signature(112, 68, "e", 0), signature(140, 90, "c", 0), signature(155, 100, "c", 0), signature(248, 68, "e", 0),
         signature(300, 101, "&", 0), signature(473, 75, "m", 0), signature(474, 79, "m", 0),
         signature(75, 63, "D", 1), signature(87, 96, "D", 1), signature(89, 94, "D", 1), signature(144, 95, "c", 1),
         signature(169, 99, "o", 1), signature(204, 99, "d", 1), signature(317, 99, "&", 1), signature(385, 99, "o", 1),
         signature(422, 99, "o", 1), signature(87, 84, "D", 2), signature(205, 88, "d", 2), signature(479, 72, "m", 2),
      ],
   }
}
