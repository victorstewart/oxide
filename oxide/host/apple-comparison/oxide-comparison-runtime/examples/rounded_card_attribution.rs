//! Deterministic comparison-only attribution for dashboard rounded-card coverage.

use std::{
   collections::BTreeMap,
   env,
   fs::File,
   io::{self, Write},
   path::{Path, PathBuf},
   process::ExitCode,
};

use serde_json::{json, Value};

const LOGICAL_WIDTH: u32 = 390;
const LOGICAL_HEIGHT: u32 = 844;
const CARD_COUNT: u32 = 32;
const CARD_WIDTH: u32 = 173;
const CARD_HEIGHT: u32 = 38;
const CARD_RADIUS: u32 = 12;
const SHADOW_OFFSET: u32 = 2;

#[derive(Clone, Copy)]
struct Rect
{
   x: f64,
   y: f64,
   width: f64,
   height: f64,
}

impl Rect
{
   fn contains(self, x: f64, y: f64) -> bool
   {
      x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
   }
}

struct Image
{
   width: u32,
   height: u32,
   rgb: Vec<u8>,
}

impl Image
{
   fn pixel(&self, x: u32, y: u32) -> [u8; 3]
   {
      let offset = ((y * self.width + x) * 3) as usize;
      [self.rgb[offset], self.rgb[offset + 1], self.rgb[offset + 2]]
   }
}

struct Arguments
{
   native: PathBuf,
   oxide: PathBuf,
   scale: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Underlay
{
   Background,
   Material,
   ShadowBackground,
   ShadowMaterial,
}

impl Underlay
{
   const ALL: [Self; 4] = [Self::Background, Self::Material, Self::ShadowBackground, Self::ShadowMaterial];

   fn id(self) -> &'static str
   {
      match self
      {
         Self::Background => "background",
         Self::Material => "material_over_background",
         Self::ShadowBackground => "shadow_over_background",
         Self::ShadowMaterial => "shadow_over_material_over_background",
      }
   }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EdgeRole
{
   SurfaceOnly,
   SurfaceOverShadow,
   ShadowOnly,
   AnalyticFringe,
}

impl EdgeRole
{
   const ALL: [Self; 4] = [Self::SurfaceOnly, Self::SurfaceOverShadow, Self::ShadowOnly, Self::AnalyticFringe];

   fn id(self) -> &'static str
   {
      match self
      {
         Self::SurfaceOnly => "surface_over_base",
         Self::SurfaceOverShadow => "surface_over_shadow",
         Self::ShadowOnly => "shadow_only",
         Self::AnalyticFringe => "outside_analytic_support",
      }
   }
}

#[derive(Default)]
struct DifferenceStats
{
   compared_pixels: u64,
   differing_pixels: u64,
   differing_channels: u64,
   max_channel_delta: u8,
   signed_delta_sum: [i64; 3],
   delta_histogram: BTreeMap<u8, u64>,
}

impl DifferenceStats
{
   fn compare(&mut self, native: [u8; 3], oxide: [u8; 3]) -> bool
   {
      self.compared_pixels += 1;
      let mut differs = false;
      for channel in 0..3
      {
         let delta = native[channel].abs_diff(oxide[channel]);
         if delta == 0
         {
            continue;
         }
         differs = true;
         self.differing_channels += 1;
         self.max_channel_delta = self.max_channel_delta.max(delta);
         self.signed_delta_sum[channel] += i64::from(native[channel]) - i64::from(oxide[channel]);
         *self.delta_histogram.entry(delta).or_default() += 1;
      }
      self.differing_pixels += u64::from(differs);
      differs
   }

   fn observe_difference(&mut self, native: [u8; 3], oxide: [u8; 3])
   {
      let _ = self.compare(native, oxide);
   }

   fn json(&self) -> Value
   {
      json!({
         "compared_pixels": self.compared_pixels,
         "differing_pixels": self.differing_pixels,
         "differing_channels": self.differing_channels,
         "max_channel_delta": self.max_channel_delta,
         "signed_native_minus_oxide": self.signed_delta_sum,
         "channel_delta_histogram": self.delta_histogram.iter().map(|(delta, count)| json!([delta, count])).collect::<Vec<_>>(),
      })
   }
}

#[derive(Default)]
struct RelativeSample
{
   occurrences: u32,
   stats: DifferenceStats,
   color_pairs: BTreeMap<(u32, u32), u32>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SampleKey
{
   underlay: Underlay,
   role: EdgeRole,
   x: u32,
   y: u32,
}

fn main() -> ExitCode
{
   match run()
   {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) =>
      {
         eprintln!("rounded-card attribution failed: {error}");
         ExitCode::FAILURE
      }
   }
}

fn run() -> Result<(), String>
{
   let arguments = arguments()?;
   let native = decode_png(&arguments.native)?;
   let oxide = decode_png(&arguments.oxide)?;
   validate_images(&native, &oxide, arguments.scale)?;
   let report = attribute(&native, &oxide, &arguments);
   let stdout = io::stdout();
   let mut output = stdout.lock();
   serde_json::to_writer(&mut output, &report).map_err(|error| format!("serializing attribution: {error}"))?;
   writeln!(output).map_err(|error| format!("writing attribution: {error}"))?;
   Ok(())
}

fn arguments() -> Result<Arguments, String>
{
   let mut native = None;
   let mut oxide = None;
   let mut scale = 3;
   let mut arguments = env::args().skip(1);
   while let Some(argument) = arguments.next()
   {
      match argument.as_str()
      {
         "--native" => native = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--oxide" => oxide = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--scale" =>
         {
            let value = arguments.next().ok_or_else(usage)?;
            scale = value.parse::<u32>().map_err(|error| format!("invalid --scale {value}: {error}"))?;
         }
         "--help" | "-h" => return Err(usage()),
         _ => return Err(format!("unknown argument {argument}\n{}", usage())),
      }
   }
   if scale == 0
   {
      return Err(String::from("--scale must be greater than zero"));
   }
   Ok(Arguments {
      native: native.ok_or_else(usage)?,
      oxide: oxide.ok_or_else(usage)?,
      scale,
   })
}

fn usage() -> String
{
   String::from("usage: rounded_card_attribution --native APPKIT.png --oxide OXIDE.png [--scale 3]")
}

fn decode_png(path: &Path) -> Result<Image, String>
{
   let file = File::open(path).map_err(|error| format!("opening {}: {error}", path.display()))?;
   let mut decoder = png::Decoder::new(file);
   decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
   let mut reader = decoder.read_info().map_err(|error| format!("reading {} metadata: {error}", path.display()))?;
   let mut source = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut source).map_err(|error| format!("decoding {}: {error}", path.display()))?;
   if info.bit_depth != png::BitDepth::Eight
   {
      return Err(format!("{} did not decode to 8-bit channels", path.display()));
   }
   let source = &source[..info.buffer_size()];
   let mut rgb = Vec::with_capacity(info.width as usize * info.height as usize * 3);
   match info.color_type
   {
      png::ColorType::Rgb => rgb.extend_from_slice(source),
      png::ColorType::Rgba =>
      {
         for pixel in source.chunks_exact(4)
         {
            if pixel[3] != 255
            {
               return Err(format!("{} contains non-opaque pixels", path.display()));
            }
            rgb.extend_from_slice(&pixel[..3]);
         }
      }
      png::ColorType::Grayscale =>
      {
         for &gray in source
         {
            rgb.extend_from_slice(&[gray, gray, gray]);
         }
      }
      png::ColorType::GrayscaleAlpha =>
      {
         for pixel in source.chunks_exact(2)
         {
            if pixel[1] != 255
            {
               return Err(format!("{} contains non-opaque pixels", path.display()));
            }
            rgb.extend_from_slice(&[pixel[0], pixel[0], pixel[0]]);
         }
      }
      png::ColorType::Indexed => return Err(format!("{} retained indexed pixels after PNG expansion", path.display())),
   }
   Ok(Image {width: info.width, height: info.height, rgb})
}

fn validate_images(native: &Image, oxide: &Image, scale: u32) -> Result<(), String>
{
   let expected_width = LOGICAL_WIDTH.checked_mul(scale).ok_or_else(|| String::from("scaled width overflow"))?;
   let expected_height = LOGICAL_HEIGHT.checked_mul(scale).ok_or_else(|| String::from("scaled height overflow"))?;
   if native.width != oxide.width || native.height != oxide.height
   {
      return Err(format!("image dimensions differ: native {}x{}, Oxide {}x{}", native.width, native.height, oxide.width, oxide.height));
   }
   if native.width != expected_width || native.height != expected_height
   {
      return Err(format!("dashboard contract requires {expected_width}x{expected_height}, found {}x{}", native.width, native.height));
   }
   Ok(())
}

fn attribute(native: &Image, oxide: &Image, arguments: &Arguments) -> Value
{
   let mut full_frame = DifferenceStats::default();
   for y in 0..native.height
   {
      for x in 0..native.width
      {
         let _ = full_frame.compare(native.pixel(x, y), oxide.pixel(x, y));
      }
   }

   let mut underlay_stats = Underlay::ALL.into_iter().map(|class| (class, DifferenceStats::default())).collect::<BTreeMap<_, _>>();
   let mut role_stats = EdgeRole::ALL.into_iter().map(|role| (role, DifferenceStats::default())).collect::<BTreeMap<_, _>>();
   let mut relative_samples = BTreeMap::<SampleKey, RelativeSample>::new();
   let mut excluded_content = DifferenceStats::default();
   for index in 0..CARD_COUNT
   {
      let card = card_rect(index, arguments.scale);
      let max_x = (card.x + card.width) as u32;
      let max_y = (card.y + card.height + f64::from(SHADOW_OFFSET * arguments.scale)) as u32;
      for y in card.y as u32..max_y
      {
         for x in card.x as u32..max_x
         {
            let native_pixel = native.pixel(x, y);
            let oxide_pixel = oxide.pixel(x, y);
            if excluded_content_kind(index, x, y, arguments.scale).is_some()
            {
               let _ = excluded_content.compare(native_pixel, oxide_pixel);
               continue;
            }
            let (underlay, role) = classify(index, x, y, arguments.scale);
            let differs = underlay_stats.entry(underlay).or_default().compare(native_pixel, oxide_pixel);
            let _ = role_stats.entry(role).or_default().compare(native_pixel, oxide_pixel);
            if !differs
            {
               continue;
            }
            let key = SampleKey {
               underlay,
               role,
               x: x - card.x as u32,
               y: y - card.y as u32,
            };
            let sample = relative_samples.entry(key).or_default();
            sample.stats.observe_difference(native_pixel, oxide_pixel);
            *sample.color_pairs.entry((pack_rgb(native_pixel), pack_rgb(oxide_pixel))).or_default() += 1;
         }
      }
   }

   for (key, sample) in &mut relative_samples
   {
      for index in 0..CARD_COUNT
      {
         let card = card_rect(index, arguments.scale);
         let x = card.x as u32 + key.x;
         let y = card.y as u32 + key.y;
         if y >= card.y as u32 + CARD_HEIGHT * arguments.scale + SHADOW_OFFSET * arguments.scale
            || excluded_content_kind(index, x, y, arguments.scale).is_some()
         {
            continue;
         }
         let (underlay, role) = classify(index, x, y, arguments.scale);
         sample.occurrences += u32::from(underlay == key.underlay && role == key.role);
      }
   }

   let class_json = Underlay::ALL.iter().map(|class| {
      let stats = underlay_stats.get(class).map(DifferenceStats::json).unwrap_or_else(|| DifferenceStats::default().json());
      json!({"id": class.id(), "stats": stats})
   }).collect::<Vec<_>>();
   let role_json = EdgeRole::ALL.iter().map(|role| {
      let stats = role_stats.get(role).map(DifferenceStats::json).unwrap_or_else(|| DifferenceStats::default().json());
      json!({"id": role.id(), "stats": stats})
   }).collect::<Vec<_>>();
   let sample_json = relative_samples.iter().map(|(key, sample)| {
      json!({
         "underlay": key.underlay.id(),
         "role": key.role.id(),
         "x": key.x,
         "y": key.y,
         "occurrences": sample.occurrences,
         "stats": sample.stats.json(),
         "color_pairs": sample.color_pairs.iter().map(|((native, oxide), count)| json!([native, oxide, count])).collect::<Vec<_>>(),
      })
   }).collect::<Vec<_>>();
   let repetition_json = relative_position_repetition(&relative_samples);

   json!({
      "schema_version": 1,
      "scenario_id": "dashboard.mixed-static",
      "checkpoint": "idle",
      "algorithm": "canonical-rounded-card-relative-rgb8-v1",
      "acceptance_note": "diagnostic attribution only; static acceptance remains exact full-frame RGB equality",
      "inputs": {
         "native_png": arguments.native.display().to_string(),
         "oxide_png": arguments.oxide.display().to_string(),
      },
      "contract": {
         "width": native.width,
         "height": native.height,
         "scale": arguments.scale,
         "cards": CARD_COUNT,
         "card_logical_rect": [0, 0, CARD_WIDTH, CARD_HEIGHT],
         "radius": CARD_RADIUS,
         "shadow_offset_y": SHADOW_OFFSET,
      },
      "full_frame": full_frame.json(),
      "excluded_content": excluded_content.json(),
      "underlay_classes": class_json,
      "edge_roles": role_json,
      "relative_position_repetition": repetition_json,
      "relative_mismatches": sample_json,
   })
}

fn relative_position_repetition(samples: &BTreeMap<SampleKey, RelativeSample>) -> Vec<Value>
{
   let mut positions = BTreeMap::<(u32, u32), u64>::new();
   for (key, sample) in samples
   {
      *positions.entry((key.x, key.y)).or_default() += sample.stats.differing_pixels;
   }
   let mut repetition = BTreeMap::<u64, (u64, u64)>::new();
   for mismatching_cards in positions.into_values()
   {
      let entry = repetition.entry(mismatching_cards).or_default();
      entry.0 += 1;
      entry.1 += mismatching_cards;
   }
   repetition.into_iter().map(|(mismatching_cards, (relative_positions, differing_pixels))| {
      json!({
         "mismatching_cards": mismatching_cards,
         "relative_positions": relative_positions,
         "differing_pixels": differing_pixels,
      })
   }).collect()
}

fn card_rect(index: u32, scale: u32) -> Rect
{
   let column = index % 2;
   let row = index / 2;
   Rect {
      x: f64::from((16 + column * 185) * scale),
      y: f64::from((48 + row * 46) * scale),
      width: f64::from(CARD_WIDTH * scale),
      height: f64::from(CARD_HEIGHT * scale),
   }
}

fn classify(index: u32, x: u32, y: u32, scale: u32) -> (Underlay, EdgeRole)
{
   let card = card_rect(index, scale);
   let shadow = Rect {y: card.y + f64::from(SHADOW_OFFSET * scale), ..card};
   let radius = f64::from(CARD_RADIUS * scale);
   let sample_x = f64::from(x) + 0.5;
   let sample_y = f64::from(y) + 0.5;
   let surface_coverage = rounded_rect_coverage(card, radius, sample_x, sample_y);
   let shadow_coverage = rounded_rect_coverage(shadow, radius, sample_x, sample_y);
   let material = is_material(sample_x, sample_y, scale);
   let underlay = match (material, shadow_coverage > 0.0)
   {
      (false, false) => Underlay::Background,
      (true, false) => Underlay::Material,
      (false, true) => Underlay::ShadowBackground,
      (true, true) => Underlay::ShadowMaterial,
   };
   let role = match (surface_coverage > 0.0, shadow_coverage > 0.0)
   {
      (true, false) => EdgeRole::SurfaceOnly,
      (true, true) => EdgeRole::SurfaceOverShadow,
      (false, true) => EdgeRole::ShadowOnly,
      (false, false) => EdgeRole::AnalyticFringe,
   };
   (underlay, role)
}

fn rounded_rect_coverage(rect: Rect, radius: f64, x: f64, y: f64) -> f64
{
   let center_x = rect.x + rect.width * 0.5;
   let center_y = rect.y + rect.height * 0.5;
   let q_x = (x - center_x).abs() - (rect.width * 0.5 - radius);
   let q_y = (y - center_y).abs() - (rect.height * 0.5 - radius);
   let outside_x = q_x.max(0.0);
   let outside_y = q_y.max(0.0);
   let distance = outside_x.hypot(outside_y) + q_x.max(q_y).min(0.0) - radius;
   (0.5 - distance).clamp(0.0, 1.0)
}

fn is_material(x: f64, y: f64, scale: u32) -> bool
{
   let scale = f64::from(scale);
   (0..4).any(|index| (Rect {
      x: 16.0 * scale,
      y: (64.0 + f64::from(index) * 204.0) * scale,
      width: 358.0 * scale,
      height: 96.0 * scale,
   }).contains(x, y))
}

fn excluded_content_kind(index: u32, x: u32, y: u32, scale: u32) -> Option<&'static str>
{
   let card = card_rect(index, scale);
   let logical_x = (f64::from(x) + 0.5 - card.x) / f64::from(scale);
   let logical_y = (f64::from(y) + 0.5 - card.y) / f64::from(scale);
   if (Rect {x: 6.0, y: 6.0, width: 14.0, height: 14.0}).contains(logical_x, logical_y)
      || (Rect {x: 24.0, y: 6.0, width: 14.0, height: 14.0}).contains(logical_x, logical_y)
   {
      return Some("icon");
   }
   if index < 24 && (Rect {x: 145.0, y: 13.0, width: 18.0, height: 12.0}).contains(logical_x, logical_y)
   {
      return Some("control");
   }
   let label_count = if index < 16 {6} else {5};
   for label in 0..label_count
   {
      if (Rect {
         x: 42.0,
         y: 2.0 - 4.0 / 3.0 + f64::from(label) * 6.0,
         width: 125.0,
         height: 6.0,
      }).contains(logical_x, logical_y)
      {
         return Some("label");
      }
   }
   None
}

fn pack_rgb(pixel: [u8; 3]) -> u32
{
   u32::from(pixel[0]) << 16 | u32::from(pixel[1]) << 8 | u32::from(pixel[2])
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn analytic_coverage_has_inside_edge_and_outside_samples()
   {
      let rect = Rect {x: 0.0, y: 0.0, width: 30.0, height: 30.0};
      assert_eq!(rounded_rect_coverage(rect, 12.0, 15.5, 15.5), 1.0);
      assert_eq!(rounded_rect_coverage(rect, 12.0, 0.5, 0.5), 0.0);
      let edge = rounded_rect_coverage(rect, 12.0, 3.5, 3.5);
      assert!(edge > 0.0 && edge < 1.0);
   }

   #[test]
   fn canonical_dashboard_exercises_all_four_underlays()
   {
      let mut seen = BTreeMap::new();
      for index in 0..CARD_COUNT
      {
         let card = card_rect(index, 3);
         for y in card.y as u32..(card.y + card.height + 6.0) as u32
         {
            for x in card.x as u32..(card.x + card.width) as u32
            {
               let (underlay, _) = classify(index, x, y, 3);
               *seen.entry(underlay).or_insert(0_u32) += 1;
            }
         }
      }
      for class in Underlay::ALL
      {
         assert!(seen.get(&class).copied().unwrap_or_default() > 0, "missing {}", class.id());
      }
   }

   #[test]
   fn difference_stats_preserve_direction_and_histogram()
   {
      let mut stats = DifferenceStats::default();
      assert!(stats.compare([10, 20, 30], [9, 22, 30]));
      assert!(!stats.compare([1, 2, 3], [1, 2, 3]));
      assert_eq!(stats.compared_pixels, 2);
      assert_eq!(stats.differing_pixels, 1);
      assert_eq!(stats.differing_channels, 2);
      assert_eq!(stats.max_channel_delta, 2);
      assert_eq!(stats.signed_delta_sum, [1, -2, 0]);
      assert_eq!(stats.delta_histogram.get(&1), Some(&1));
      assert_eq!(stats.delta_histogram.get(&2), Some(&1));
   }

   #[test]
   fn repetition_histogram_folds_underlay_rows_to_one_card_position()
   {
      let mut samples = BTreeMap::new();
      let first = SampleKey {underlay: Underlay::Background, role: EdgeRole::SurfaceOnly, x: 2, y: 3};
      let second = SampleKey {underlay: Underlay::ShadowMaterial, role: EdgeRole::SurfaceOverShadow, x: 2, y: 3};
      let third = SampleKey {underlay: Underlay::Material, role: EdgeRole::SurfaceOnly, x: 7, y: 9};
      let mut first_sample = RelativeSample::default();
      first_sample.stats.differing_pixels = 10;
      let mut second_sample = RelativeSample::default();
      second_sample.stats.differing_pixels = 22;
      let mut third_sample = RelativeSample::default();
      third_sample.stats.differing_pixels = 4;
      samples.insert(first, first_sample);
      samples.insert(second, second_sample);
      samples.insert(third, third_sample);
      assert_eq!(relative_position_repetition(&samples), vec![
         json!({"mismatching_cards": 4, "relative_positions": 1, "differing_pixels": 4}),
         json!({"mismatching_cards": 32, "relative_positions": 1, "differing_pixels": 32}),
      ]);
   }

   #[test]
   fn content_mask_separates_labels_icons_and_controls_from_card_edges()
   {
      let card = card_rect(0, 3);
      assert_eq!(excluded_content_kind(0, card.x as u32 + 18, card.y as u32 + 18, 3), Some("icon"));
      assert_eq!(excluded_content_kind(0, card.x as u32 + 130, card.y as u32 + 4, 3), Some("label"));
      assert_eq!(excluded_content_kind(0, card.x as u32 + 440, card.y as u32 + 40, 3), Some("control"));
      assert_eq!(excluded_content_kind(0, card.x as u32, card.y as u32, 3), None);
   }
}
