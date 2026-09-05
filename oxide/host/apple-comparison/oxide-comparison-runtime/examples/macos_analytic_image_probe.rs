//! Comparison-only probe for the macOS analytic rounded-image shader path.

use std::{env, fs::File, path::PathBuf, process::ExitCode};

use serde_json::json;

const SURFACE: [f32; 3] = [1.0; 3];

struct Image
{
   width: u32,
   height: u32,
   rgba: Vec<u8>,
}

impl Image
{
   fn pixel(&self, x: u32, y: u32) -> [u8; 4]
   {
      let offset = ((y * self.width + x) * 4) as usize;
      self.rgba[offset..offset + 4].try_into().unwrap()
   }
}

#[derive(Default)]
struct Difference
{
   pixels: u64,
   channels: u64,
   maximum: u8,
   samples: Vec<serde_json::Value>,
}

impl Difference
{
   fn observe(&mut self, x: u32, y: u32, actual: [u8; 4], predicted: [u8; 3])
   {
      let mut differs = false;
      for channel in 0..3
      {
         let delta = actual[channel].abs_diff(predicted[channel]);
         if delta > 0
         {
            differs = true;
            self.channels += 1;
            self.maximum = self.maximum.max(delta);
         }
      }
      self.pixels += u64::from(differs);
      if differs && self.samples.len() < 24
      {
         self.samples.push(json!({"x": x, "y": y, "actual": &actual[..3], "predicted": predicted}));
      }
   }
}

fn main() -> ExitCode
{
   match run()
   {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) =>
      {
         eprintln!("macOS analytic image probe failed: {error}");
         ExitCode::FAILURE
      }
   }
}

fn run() -> Result<(), String>
{
   let (oxide_path, atlas_path, decode_path, reference_path) = arguments()?;
   let oxide = decode_png(&oxide_path)?;
   let atlas = decode_png(&atlas_path)?;
   let reference = reference_path.as_ref().map(decode_png).transpose()?;
   let decode = decode_table(decode_path.as_ref())?;
   if oxide.width != 1_170 || oxide.height != 2_532
   {
      return Err(format!("expected 1170x2532 Oxide image, found {}x{}", oxide.width, oxide.height));
   }
   if atlas.width != 384 || atlas.height != 192
   {
      return Err(format!("expected 384x192 atlas, found {}x{}", atlas.width, atlas.height));
   }
   let heights = [68_u32, 85, 102, 119, 83, 100, 117, 81, 98];
   let mut card_y = 168_u32;
   let mut difference = Difference::default();
   let mut pair_difference = Difference::default();
   let mut compared = 0_u64;
   for (tile, height) in heights.into_iter().enumerate()
   {
      let origin_x = 66_u32;
      let origin_y = card_y + 30;
      for local_y in 0..144_u32
      {
         let y = origin_y + local_y;
         if y >= oxide.height {continue}
         for local_x in 0..144_u32
         {
            if local_x >= 24 && local_x < 120 && local_y >= 24 && local_y < 120 {continue}
            let x = origin_x + local_x;
            let predicted = predict(&atlas, &decode, tile as u32, origin_x, origin_y, x, y);
            difference.observe(x, y, oxide.pixel(x, y), predicted);
            if let Some(reference) = &reference
            {
               pair_difference.observe(x, y, reference.pixel(x, y), oxide.pixel(x, y)[..3].try_into().unwrap());
            }
            compared += 1;
         }
      }
      card_y += height * 3;
   }
   println!("{}", json!({
      "schema_version": 1,
      "algorithm": "macos-analytic-rounded-image-probe-v1",
      "oxide_png": oxide_path,
      "atlas_png": atlas_path,
      "decode_table": decode_path,
      "reference_png": reference_path,
      "compared_pixels": compared,
      "differing_pixels": difference.pixels,
      "differing_channels": difference.channels,
      "maximum_channel_delta": difference.maximum,
      "samples": difference.samples,
      "reference_difference": reference.as_ref().map(|_| json!({
         "differing_pixels": pair_difference.pixels,
         "differing_channels": pair_difference.channels,
         "maximum_channel_delta": pair_difference.maximum,
         "samples": pair_difference.samples,
      })),
   }));
   Ok(())
}

fn arguments() -> Result<(PathBuf, PathBuf, Option<PathBuf>, Option<PathBuf>), String>
{
   let mut arguments = env::args().skip(1);
   let mut oxide = None;
   let mut atlas = None;
   let mut decode = None;
   let mut reference = None;
   while let Some(argument) = arguments.next()
   {
      match argument.as_str()
      {
         "--oxide" => oxide = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--atlas" => atlas = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--decode-table" => decode = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--reference" => reference = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--help" | "-h" => return Err(usage()),
         _ => return Err(format!("unknown argument {argument}\n{}", usage())),
      }
   }
   Ok((oxide.ok_or_else(usage)?, atlas.ok_or_else(usage)?, decode, reference))
}

fn usage() -> String
{
   String::from("usage: macos_analytic_image_probe --oxide OXIDE.png --atlas ATLAS.png [--decode-table METAL.json] [--reference REFERENCE.png]")
}

fn decode_table(path: Option<&PathBuf>) -> Result<Vec<f32>, String>
{
   let Some(path) = path else {return Ok((0..=255).map(srgb8_to_linear).collect())};
   let file = File::open(path).map_err(|error| format!("opening {}: {error}", path.display()))?;
   let value: serde_json::Value = serde_json::from_reader(file).map_err(|error| format!("decoding {}: {error}", path.display()))?;
   let patterns = value.get("rgba32float_red_bit_patterns").and_then(serde_json::Value::as_array)
      .ok_or_else(|| format!("{} lacks rgba32float_red_bit_patterns", path.display()))?;
   if patterns.len() != 256 {return Err(format!("{} has {} decode entries", path.display(), patterns.len()))}
   patterns.iter().map(|pattern| {
      let bits = pattern.as_u64().and_then(|bits| u32::try_from(bits).ok())
         .ok_or_else(|| format!("{} has an invalid decode entry", path.display()))?;
      Ok(f32::from_bits(bits))
   }).collect()
}

fn decode_png(path: &PathBuf) -> Result<Image, String>
{
   let file = File::open(path).map_err(|error| format!("opening {}: {error}", path.display()))?;
   let mut decoder = png::Decoder::new(file);
   decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
   let mut reader = decoder.read_info().map_err(|error| format!("reading {}: {error}", path.display()))?;
   let mut source = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut source).map_err(|error| format!("decoding {}: {error}", path.display()))?;
   let source = &source[..info.buffer_size()];
   let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
   match info.color_type
   {
      png::ColorType::Rgb =>
      {
         for pixel in source.chunks_exact(3)
         {
            rgba.extend_from_slice(pixel);
            rgba.push(255);
         }
      }
      png::ColorType::Rgba => rgba.extend_from_slice(source),
      _ => return Err(format!("unsupported PNG color type {:?}", info.color_type)),
   }
   Ok(Image {width: info.width, height: info.height, rgba})
}

fn predict(atlas: &Image, decode: &[f32], tile: u32, origin_x: u32, origin_y: u32, x: u32, y: u32) -> [u8; 3]
{
   let local_x = x as f32 + 0.5 - origin_x as f32;
   let local_y = y as f32 + 0.5 - origin_y as f32;
   let source_x = (local_x / 6.0).clamp(0.5, 23.5);
   let source_y = (local_y / 6.0).clamp(0.5, 23.5);
   let sampled = bilinear(atlas, decode, tile, source_x, source_y);
   let coverage = coverage(origin_x, origin_y, x, y);
   let alpha = sampled[3] * coverage;
   core::array::from_fn(|channel| {
      linear_to_srgb8(sampled[channel] * alpha + SURFACE[channel] * (1.0 - alpha))
   })
}

fn bilinear(atlas: &Image, decode: &[f32], tile: u32, source_x: f32, source_y: f32) -> [f32; 4]
{
   let texel_x = source_x - 0.5;
   let texel_y = source_y - 0.5;
   let x0 = texel_x.floor().clamp(0.0, 23.0) as u32;
   let y0 = texel_y.floor().clamp(0.0, 23.0) as u32;
   let x1 = (x0 + 1).min(23);
   let y1 = (y0 + 1).min(23);
   let fx = texel_x - texel_x.floor();
   let fy = texel_y - texel_y.floor();
   let tile_x = (tile % 16) * 24;
   let tile_y = (tile / 16) * 24;
   let a = linear_pixel(atlas.pixel(tile_x + x0, tile_y + y0), decode);
   let b = linear_pixel(atlas.pixel(tile_x + x1, tile_y + y0), decode);
   let c = linear_pixel(atlas.pixel(tile_x + x0, tile_y + y1), decode);
   let d = linear_pixel(atlas.pixel(tile_x + x1, tile_y + y1), decode);
   core::array::from_fn(|channel| {
      let top = a[channel] + (b[channel] - a[channel]) * fx;
      let bottom = c[channel] + (d[channel] - c[channel]) * fx;
      top + (bottom - top) * fy
   })
}

fn linear_pixel(pixel: [u8; 4], decode: &[f32]) -> [f32; 4]
{
   [decode[pixel[0] as usize], decode[pixel[1] as usize], decode[pixel[2] as usize], f32::from(pixel[3]) / 255.0]
}

fn coverage(origin_x: u32, origin_y: u32, x: u32, y: u32) -> f32
{
   let distance = |sample_x: f32, sample_y: f32| {
      let center_x = origin_x as f32 + 72.0;
      let center_y = origin_y as f32 + 72.0;
      let q_x = (sample_x - center_x).abs() - 48.0;
      let q_y = (sample_y - center_y).abs() - 48.0;
      let outside_x = q_x.max(0.0);
      let outside_y = q_y.max(0.0);
      (outside_x * outside_x + outside_y * outside_y).sqrt() + q_x.max(q_y).min(0.0) - 24.0
   };
   let sample_x = x as f32 + 0.5;
   let sample_y = y as f32 + 0.5;
   let signed_distance = distance(sample_x, sample_y);
   let quad_x = (x & !1) as f32;
   let quad_y = (y & !1) as f32;
   let dx = distance(quad_x + 1.5, sample_y) - distance(quad_x + 0.5, sample_y);
   let dy = distance(sample_x, quad_y + 1.5) - distance(sample_x, quad_y + 0.5);
   (0.5 - signed_distance / (dx.abs() + dy.abs()).max(0.0001)).clamp(0.0, 1.0)
}

fn srgb8_to_linear(value: u8) -> f32
{
   let encoded = f64::from(value) / 255.0;
   if encoded <= 0.04045 {(encoded / 12.92) as f32} else {(((encoded + 0.055) / 1.055).powf(2.4)) as f32}
}

fn linear_to_srgb8(value: f32) -> u8
{
   let value = f64::from(value.clamp(0.0, 1.0));
   let encoded = if value <= 0.0031308 {value * 12.92} else {1.055 * value.powf(1.0 / 2.4) - 0.055};
   (encoded * 255.0).round_ties_even().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests
{
   #[test]
   fn half_texel_clamp_keeps_the_first_three_destination_pixels_on_the_edge_texel()
   {
      for local in 0..3
      {
         assert_eq!(((local as f32 + 0.5) / 6.0).clamp(0.5, 23.5), 0.5);
      }
      assert!(((3.5_f32 / 6.0).clamp(0.5, 23.5) - 0.5).abs() > 0.0);
   }
}
