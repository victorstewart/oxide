use oxide_benchmark_spec::{load_release_candidate_for_capture, validate_apple_pr_scenario_set, AssetManifest, FontPackManifest, FontVariationAxis, ImageDecodeZoomFixture, InlineTextAtlas, ReleaseCandidateCaptureSpec, ScenarioSpec, TraceEvent, APPLE_PR_SCENARIO_IDS};
use std::path::Path;

pub(crate) enum ComparisonScenario
{
   Runnable(ScenarioSpec),
   ReleaseCandidate(String),
}

impl ComparisonScenario
{
   pub fn id(&self) -> &str
   {
      match self
      {
         Self::Runnable(scenario) => &scenario.id,
         Self::ReleaseCandidate(scenario_id) => scenario_id,
      }
   }
}

pub(crate) struct ComparisonPackage
{
   pub scenario: ComparisonScenario,
   pub fixture: Vec<u8>,
   pub traces: Vec<(String, Vec<TraceEvent>)>,
   pub atlas: Option<(u32, u32, Vec<u8>)>,
   pub inline_text: Option<ComparisonInlineTextPackage>,
   pub latin_font: ComparisonFontPackage,
   pub arabic_font: ComparisonFontPackage,
   pub cjk_font: ComparisonFontPackage,
   pub image: Option<ComparisonImagePackage>,
}

pub(crate) struct ComparisonInlineTextPackage
{
   pub rasters: Vec<ComparisonInlineTextRasterPackage>,
   pub atlas: InlineTextAtlas,
}

pub(crate) struct ComparisonInlineTextRasterPackage
{
   pub width: u32,
   pub height: u32,
   pub bgra: Vec<u8>,
}

pub(crate) struct ComparisonFontPackage
{
   pub bytes: Vec<u8>,
   pub variations: Vec<FontVariationAxis>,
}

pub(crate) struct ComparisonImagePackage
{
   pub source_png: Vec<u8>,
   pub source_sha256: String,
   pub source_width: u32,
   pub source_height: u32,
   pub thumbnail_width: u32,
   pub thumbnail_height: u32,
   pub thumbnail_bgra: Vec<u8>,
   pub thumbnail_sha256: String,
}

pub(crate) fn load(root: &Path, scenario_id: &str) -> Result<ComparisonPackage, String>
{
   let mut scenarios = Vec::with_capacity(APPLE_PR_SCENARIO_IDS.len());
   for id in APPLE_PR_SCENARIO_IDS
   {
      let path = root.join("scenarios").join(format!("{id}.json"));
      scenarios.push((path.clone(), read_json::<ScenarioSpec>(&path)?));
   }
   validate_apple_pr_scenario_set(root, &scenarios).map_err(|error| error.to_string())?;
   let scenario = scenarios.into_iter().find_map(|(_, scenario)| (scenario.id == scenario_id).then_some(scenario)).ok_or_else(|| format!("unsupported comparison scenario {scenario_id}"))?;
   load_package(root, ComparisonScenario::Runnable(scenario), None)
}

pub(crate) fn load_release_candidate(root: &Path, scenario_id: &str) -> Result<ComparisonPackage, String>
{
   let candidate = load_release_candidate_for_capture(root, scenario_id).map_err(|error| error.to_string())?;
   load_package(root, ComparisonScenario::ReleaseCandidate(candidate.id.clone()), Some(candidate))
}

fn load_package(root: &Path, scenario: ComparisonScenario, candidate: Option<ReleaseCandidateCaptureSpec>) -> Result<ComparisonPackage, String>
{
   let (scenario_id, fixture_identity, assets_identity, font_pack, phases) = match (&scenario, candidate.as_ref())
   {
      (ComparisonScenario::Runnable(scenario), None) => (scenario.id.as_str(), &scenario.fixture, &scenario.assets, &scenario.font_pack, scenario.phases.as_slice()),
      (ComparisonScenario::ReleaseCandidate(scenario_id), Some(candidate)) => (scenario_id.as_str(), &candidate.fixture, &candidate.assets, &candidate.font_pack, candidate.phases.as_slice()),
      _ => return Err(String::from("comparison package kind differs from its validated contract")),
   };
   let fixture = std::fs::read(root.join(&fixture_identity.path)).map_err(|error| format!("reading fixture {}: {error}", fixture_identity.path))?;
   let asset_manifest: AssetManifest = read_json(&root.join(&assets_identity.path))?;
   let atlas = if let Some(atlas) = asset_manifest.artifacts.iter().find(|asset| asset.role == "thumbnail-atlas")
   {
      let bytes = std::fs::read(root.join(&atlas.artifact.path)).map_err(|error| format!("reading atlas {}: {error}", atlas.artifact.path))?;
      Some(decode_png_bgra(&bytes).map_err(|()| String::from("decoding comparison thumbnail atlas"))?)
   }
   else if scenario_id == "image.decode-zoom"
   {
      None
   }
   else
   {
      return Err(String::from("comparison asset manifest has no thumbnail-atlas"));
   };
   let inline_text = if let Some(contract) = asset_manifest.inline_text_atlas
   {
      let mut rasters = Vec::with_capacity(contract.variants.len());
      for variant in &contract.variants
      {
         let artifact = asset_manifest.artifacts.iter().find(|artifact| artifact.role == variant.artifact_role).ok_or_else(|| format!("comparison inline-text raster {} is missing", variant.artifact_role))?;
         let bytes = std::fs::read(root.join(&artifact.artifact.path)).map_err(|error| format!("reading inline-text raster {}: {error}", artifact.artifact.path))?;
         let (width, height, bgra) = decode_png_bgra(&bytes).map_err(|()| format!("decoding comparison inline-text raster {}", artifact.artifact.path))?;
         if width != variant.pixel_width || height != variant.pixel_height
         {
            return Err(format!("comparison inline-text raster dimensions {width}x{height} do not match contract {}x{}", variant.pixel_width, variant.pixel_height));
         }
         rasters.push(ComparisonInlineTextRasterPackage {width, height, bgra});
      }
      Some(ComparisonInlineTextPackage {rasters, atlas: contract})
   }
   else
   {
      None
   };
   let font_manifest: FontPackManifest = read_json(&root.join(&font_pack.manifest))?;
   let font = |role: &str| -> Result<ComparisonFontPackage, String> {
      let font = font_manifest.fonts.iter().find(|font| font.role == role).ok_or_else(|| format!("comparison font pack has no {role} font"))?;
      let bytes = std::fs::read(root.join(&font.artifact.path)).map_err(|error| format!("reading comparison font {}: {error}", font.artifact.path))?;
      Ok(ComparisonFontPackage {bytes, variations: font.variation_axes.clone()})
   };
   let mut traces = Vec::new();
   for phase in phases
   {
      if let Some(trace) = &phase.trace
      {
         traces.push((phase.id.clone(), read_json::<Vec<TraceEvent>>(&root.join(&trace.path))?));
      }
   }
   let image = if scenario_id == "image.decode-zoom"
   {
      let image: ImageDecodeZoomFixture = serde_json::from_slice(&fixture).map_err(|error| format!("parsing image fixture: {error}"))?;
      let source_png = std::fs::read(root.join(&image.source.artifact.path)).map_err(|error| format!("reading comparison source image {}: {error}", image.source.artifact.path))?;
      let thumbnail_png = std::fs::read(root.join(&image.thumbnail.artifact.path)).map_err(|error| format!("reading comparison thumbnail {}: {error}", image.thumbnail.artifact.path))?;
      let (thumbnail_width, thumbnail_height, thumbnail_bgra) = decode_png_bgra(&thumbnail_png).map_err(|()| String::from("decoding comparison thumbnail"))?;
      Some(ComparisonImagePackage {
         source_png,
         source_sha256: image.source.artifact.sha256,
         source_width: image.source.width,
         source_height: image.source.height,
         thumbnail_width,
         thumbnail_height,
         thumbnail_bgra,
         thumbnail_sha256: image.thumbnail.artifact.sha256,
      })
   }
   else
   {
      None
   };
   Ok(ComparisonPackage {
      scenario,
      fixture,
      traces,
      atlas,
      inline_text,
      latin_font: font("latin")?,
      arabic_font: font("arabic")?,
      cjk_font: font("cjk-simplified")?,
      image,
   })
}

pub(crate) fn decode_png_bgra(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ()>
{
   let decoder = png::Decoder::new(bytes);
   let mut reader = decoder.read_info().map_err(|_| ())?;
   let mut buffer = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut buffer).map_err(|_| ())?;
   let width = info.width;
   let height = info.height;
   buffer.truncate(info.buffer_size());
   let mut rgba = match info.color_type
   {
      png::ColorType::Rgba => buffer,
      png::ColorType::Rgb =>
      {
         let mut output = Vec::with_capacity((width * height * 4) as usize);
         for pixel in buffer.chunks_exact(3)
         {
            output.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
         }
         output
      }
      png::ColorType::Grayscale => buffer.iter().flat_map(|&gray| [gray, gray, gray, 255]).collect(),
      png::ColorType::GrayscaleAlpha =>
      {
         let mut output = Vec::with_capacity((width * height * 4) as usize);
         for pixel in buffer.chunks_exact(2)
         {
            output.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
         }
         output
      }
      _ => return Err(()),
   };
   for pixel in rgba.chunks_exact_mut(4)
   {
      pixel.swap(0, 2);
   }
   Ok((width, height, rgba))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String>
{
   let bytes = std::fs::read(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
   serde_json::from_slice(&bytes).map_err(|error| format!("parsing {}: {error}", path.display()))
}
