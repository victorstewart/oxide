//! Comparison-only AppKit/Metal image sampling diagnostic.

use std::{
   error::Error,
   fmt,
   fs,
   io::Cursor,
   path::{Path, PathBuf},
   process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Width of the frozen source fixture.
pub const SOURCE_WIDTH: u32 = 4_096;
/// Height of the frozen source fixture.
pub const SOURCE_HEIGHT: u32 = 3_072;
/// Width of each bounded comparison region.
pub const ROI_WIDTH: u32 = 256;
/// Height of each bounded comparison region.
pub const ROI_HEIGHT: u32 = 192;
/// Physical-pixel phases exercised by every sampling ratio.
pub const PHASES_MILLIONTHS: [i32; 3] = [0, 250_000, 500_000];

/// One immutable full-source-to-destination geometry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SamplingCase
{
   /// Stable report identity.
   pub id: &'static str,
   /// Full destination width expressed in millionths of a physical pixel.
   pub destination_width_millionths: u64,
   /// Full destination height expressed in millionths of a physical pixel.
   pub destination_height_millionths: u64,
}

/// Complete bounded request consumed by the AppKit/Metal helper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SamplingRequest
{
   /// Request schema identity.
   pub schema_version: u32,
   /// Frozen source width.
   pub source_width: u32,
   /// Frozen source height.
   pub source_height: u32,
   /// Captured region width.
   pub roi_width: u32,
   /// Captured region height.
   pub roi_height: u32,
   /// Requested destination phases.
   pub phases_millionths: [i32; 3],
   /// Requested sampling ratios.
   pub cases: Vec<SamplingCase>,
}

/// Validated identity fields returned after a complete diagnostic run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SamplingDiagnosticSummary
{
   /// Report schema identity.
   pub schema_version: u32,
   /// Exact reducer identity.
   pub algorithm: String,
   /// Encoded source artifact identity.
   pub source_png_sha256: String,
   /// Rust-decoded canonical RGBA identity.
   pub oxide_decoded_rgba_sha256: String,
   /// AppKit-decoded canonical RGBA identity.
   pub appkit_decoded_rgba_sha256: String,
   /// Whether Rust and AppKit produced identical decoded RGBA bytes.
   pub decoded_rgba_exact: bool,
   /// Number of AppKit/Metal pairs in the report.
   pub pair_count: usize,
   /// Persisted report location.
   pub report_path: PathBuf,
}

/// Fail-closed diagnostic setup, tool, or report-contract error.
#[derive(Debug)]
pub struct SamplingDiagnosticError(String);

impl SamplingDiagnosticError
{
   fn new(message: impl Into<String>) -> Self
   {
      Self(message.into())
   }
}

impl fmt::Display for SamplingDiagnosticError
{
   fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      formatter.write_str(&self.0)
   }
}

impl Error for SamplingDiagnosticError {}

/// Returns the frozen four-ratio, three-phase request.
pub fn sampling_request() -> SamplingRequest
{
   SamplingRequest {
      schema_version: 1,
      source_width: SOURCE_WIDTH,
      source_height: SOURCE_HEIGHT,
      roi_width: ROI_WIDTH,
      roi_height: ROI_HEIGHT,
      phases_millionths: PHASES_MILLIONTHS,
      cases: vec![
         SamplingCase {
            id: "one-to-one",
            destination_width_millionths: 4_096_000_000,
            destination_height_millionths: 3_072_000_000,
         },
         SamplingCase {
            id: "integer-two-to-one",
            destination_width_millionths: 2_048_000_000,
            destination_height_millionths: 1_536_000_000,
         },
         SamplingCase {
            id: "current-3.50-to-one",
            destination_width_millionths: 1_170_000_000,
            destination_height_millionths: 877_500_000,
         },
         SamplingCase {
            id: "current-1.75-to-one",
            destination_width_millionths: 2_340_000_000,
            destination_height_millionths: 1_755_000_000,
         },
      ],
   }
}

/// Decodes supported PNG color layouts into tightly packed canonical RGBA8.
pub fn decode_canonical_rgba(source_png: &[u8]) -> Result<(u32, u32, Vec<u8>), SamplingDiagnosticError>
{
   let mut decoder = png::Decoder::new(Cursor::new(source_png));
   decoder.set_transformations(png::Transformations::normalize_to_color8());
   let mut reader = decoder.read_info().map_err(|error| SamplingDiagnosticError::new(format!("decoding PNG header: {error}")))?;
   let mut decoded = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut decoded).map_err(|error| SamplingDiagnosticError::new(format!("decoding PNG pixels: {error}")))?;
   let decoded = &decoded[..info.buffer_size()];
   let pixel_count = (info.width as usize).checked_mul(info.height as usize).ok_or_else(|| SamplingDiagnosticError::new("PNG dimensions overflow address space"))?;
   let rgba_capacity = pixel_count.checked_mul(4).ok_or_else(|| SamplingDiagnosticError::new("RGBA byte count overflows address space"))?;
   let mut rgba = Vec::with_capacity(rgba_capacity);
   match info.color_type
   {
      png::ColorType::Rgb =>
      {
         for pixel in decoded.chunks_exact(3)
         {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
         }
      }
      png::ColorType::Rgba => rgba.extend_from_slice(decoded),
      png::ColorType::Grayscale =>
      {
         for value in decoded
         {
            rgba.extend_from_slice(&[*value, *value, *value, 255]);
         }
      }
      png::ColorType::GrayscaleAlpha =>
      {
         for pixel in decoded.chunks_exact(2)
         {
            rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
         }
      }
      png::ColorType::Indexed => return Err(SamplingDiagnosticError::new("PNG remained indexed after color8 normalization")),
   }
   if rgba.len() != rgba_capacity
   {
      return Err(SamplingDiagnosticError::new("decoded RGBA byte count differs from PNG dimensions"));
   }
   Ok((info.width, info.height, rgba))
}

/// Returns a lowercase SHA-256 identity.
pub fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

/// Persists the request, decoded source evidence, and complete offscreen report.
pub fn run(source_path: &Path, output_directory: &Path) -> Result<SamplingDiagnosticSummary, SamplingDiagnosticError>
{
   let source_png = fs::read(source_path).map_err(|error| SamplingDiagnosticError::new(format!("reading {}: {error}", source_path.display())))?;
   let source_png_sha256 = sha256(&source_png);
   let (width, height, rgba) = decode_canonical_rgba(&source_png)?;
   if width != SOURCE_WIDTH || height != SOURCE_HEIGHT
   {
      return Err(SamplingDiagnosticError::new(format!("source is {width}x{height}; expected {SOURCE_WIDTH}x{SOURCE_HEIGHT}")));
   }
   if rgba.chunks_exact(4).any(|pixel| pixel[3] != 255)
   {
      return Err(SamplingDiagnosticError::new("source contains nonopaque pixels"));
   }

   fs::create_dir_all(output_directory).map_err(|error| SamplingDiagnosticError::new(format!("creating {}: {error}", output_directory.display())))?;
   let request_path = output_directory.join("sampling-request.json");
   let oxide_bgra_path = output_directory.join("oxide-decoded-source.bgra");
   let report_path = output_directory.join("report.json");
   let request = serde_json::to_vec_pretty(&sampling_request()).map_err(|error| SamplingDiagnosticError::new(format!("encoding sampling request: {error}")))?;
   fs::write(&request_path, request).map_err(|error| SamplingDiagnosticError::new(format!("writing {}: {error}", request_path.display())))?;

   let oxide_decoded_rgba_sha256 = sha256(&rgba);
   let mut bgra = rgba;
   for pixel in bgra.chunks_exact_mut(4)
   {
      pixel.swap(0, 2);
   }
   fs::write(&oxide_bgra_path, &bgra).map_err(|error| SamplingDiagnosticError::new(format!("writing {}: {error}", oxide_bgra_path.display())))?;

   let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("AppKitImageSamplingReference.swift");
   let output = Command::new("xcrun")
      .arg("swift")
      .arg(&helper)
      .arg(source_path)
      .arg(&oxide_bgra_path)
      .arg(&request_path)
      .arg(&report_path)
      .arg(&source_png_sha256)
      .arg(&oxide_decoded_rgba_sha256)
      .output()
      .map_err(|error| SamplingDiagnosticError::new(format!("launching AppKit reference helper: {error}")))?;
   if !output.status.success()
   {
      let stderr = String::from_utf8_lossy(&output.stderr);
      return Err(SamplingDiagnosticError::new(format!("AppKit reference helper failed with {}: {stderr}", output.status)));
   }

   let report_bytes = fs::read(&report_path).map_err(|error| SamplingDiagnosticError::new(format!("reading {}: {error}", report_path.display())))?;
   let mut summary = serde_json::from_slice::<SamplingDiagnosticSummary>(&report_bytes).map_err(|error| SamplingDiagnosticError::new(format!("parsing diagnostic report: {error}")))?;
   if summary.schema_version != 1 || summary.algorithm != "appkit-metal-image-sampling-exact-v1"
   {
      return Err(SamplingDiagnosticError::new("diagnostic report identity is unsupported"));
   }
   if summary.source_png_sha256 != source_png_sha256 || summary.oxide_decoded_rgba_sha256 != oxide_decoded_rgba_sha256
   {
      return Err(SamplingDiagnosticError::new("diagnostic report source identity differs from the Rust decoder"));
   }
   if summary.pair_count != sampling_request().cases.len() * PHASES_MILLIONTHS.len() * 3
   {
      return Err(SamplingDiagnosticError::new("diagnostic report pair count is incomplete"));
   }
   summary.report_path = report_path;
   Ok(summary)
}
