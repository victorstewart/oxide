//! Comparison-only probe for the macOS analytic rounded-rectangle shader path.

use std::{env, fs::File, path::PathBuf, process::ExitCode};

use serde_json::json;

const BACKGROUND: [f32; 3] = [0.896269353, 0.913098652, 0.938685728];
const SHADOW: [f32; 4] = [0.014443844, 0.017641954, 0.025186860, 0.160784314];
const SURFACE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[derive(Clone, Copy)]
struct Rect
{
   x: f32,
   y: f32,
   width: f32,
   height: f32,
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

#[derive(Clone, Copy)]
enum Derivative
{
   QuadFine,
   Central,
   Unit,
}

impl Derivative
{
   const ALL: [Self; 3] = [Self::QuadFine, Self::Central, Self::Unit];

   fn id(self) -> &'static str
   {
      match self
      {
         Self::QuadFine => "quad-fine",
         Self::Central => "central",
         Self::Unit => "unit",
      }
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
   fn observe(&mut self, x: u32, y: u32, actual: [u8; 3], predicted: [u8; 3])
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
      if differs && self.samples.len() < 16
      {
         self.samples.push(json!({"x": x, "y": y, "actual": actual, "predicted": predicted}));
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
         eprintln!("macOS analytic edge probe failed: {error}");
         ExitCode::FAILURE
      }
   }
}

fn run() -> Result<(), String>
{
   let path = arguments()?;
   let image = decode_png(&path)?;
   if image.width != 1_170 || image.height != 2_532
   {
      return Err(format!("expected 1170x2532 image, found {}x{}", image.width, image.height));
   }
   let heights = [68_u32, 85, 102, 119, 83, 100, 117, 81, 98];
   let mut origin_y = 168.0_f32;
   let cards = heights.into_iter().map(|height| {
      let card = Rect {x: 36.0, y: origin_y, width: 1_098.0, height: (height - 8) as f32 * 3.0};
      origin_y += height as f32 * 3.0;
      card
   }).collect::<Vec<_>>();
   let cases = Derivative::ALL.into_iter().map(|derivative| {
      let mut difference = Difference::default();
      let mut compared = 0_u64;
      for card in &cards
      {
         let shadow = Rect {y: card.y + 6.0, ..*card};
         for y in card.y as u32..((card.y + card.height + 6.0) as u32).min(image.height)
         {
            for x in card.x as u32..(card.x + card.width) as u32
            {
               let relative_x = x - card.x as u32;
               let relative_y = y - card.y as u32;
               let in_corner = (relative_x < 30 || relative_x >= card.width as u32 - 36)
                  && (relative_y < 36 || relative_y >= card.height as u32 - 36);
               if !in_corner
               {
                  continue;
               }
               let predicted = compose(*card, shadow, x, y, derivative);
               difference.observe(x, y, image.pixel(x, y), predicted);
               compared += 1;
            }
         }
      }
      json!({
         "derivative": derivative.id(),
         "compared_pixels": compared,
         "differing_pixels": difference.pixels,
         "differing_channels": difference.channels,
         "maximum_channel_delta": difference.maximum,
         "samples": difference.samples,
      })
   }).collect::<Vec<_>>();
   let card = cards[0];
   let diagnostic_coverage = coverage(card, 36.0, 61, 169, Derivative::QuadFine);
   let diagnostic_linear = SURFACE[0] * diagnostic_coverage + BACKGROUND[0] * (1.0 - diagnostic_coverage);
   let diagnostic_encoded = if diagnostic_linear <= 0.0031308
   {
      diagnostic_linear * 12.92
   }
   else
   {
      1.055 * diagnostic_linear.powf(1.0 / 2.4) - 0.055
   };
   println!("{}", json!({
      "schema_version": 1,
      "algorithm": "macos-analytic-rounded-edge-probe-v1",
      "oxide_png": path,
      "card_physical_rects": cards.iter().map(|card| [card.x, card.y, card.width, card.height]).collect::<Vec<_>>(),
      "diagnostic": {
         "coordinate": [61, 169],
         "coverage": diagnostic_coverage,
         "red_linear": diagnostic_linear,
         "red_encoded_255": diagnostic_encoded * 255.0,
      },
      "cases": cases,
   }));
   Ok(())
}

fn arguments() -> Result<PathBuf, String>
{
   let mut arguments = env::args().skip(1);
   let mut oxide = None;
   while let Some(argument) = arguments.next()
   {
      match argument.as_str()
      {
         "--oxide" => oxide = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--help" | "-h" => return Err(usage()),
         _ => return Err(format!("unknown argument {argument}\n{}", usage())),
      }
   }
   oxide.ok_or_else(usage)
}

fn usage() -> String
{
   String::from("usage: macos_analytic_edge_probe --oxide OXIDE.png")
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
               return Err(String::from("Oxide screenshot is not opaque"));
            }
            rgb.extend_from_slice(&pixel[..3]);
         }
      }
      _ => return Err(format!("unsupported Oxide PNG color type {:?}", info.color_type)),
   }
   Ok(Image {width: info.width, height: info.height, rgb})
}

fn compose(card: Rect, shadow: Rect, x: u32, y: u32, derivative: Derivative) -> [u8; 3]
{
   let mut encoded = BACKGROUND.map(linear_to_srgb8);
   let shadow_coverage = coverage(shadow, 36.0, x, y, derivative);
   encoded = blend(encoded, SHADOW, shadow_coverage);
   let surface_coverage = coverage(card, 36.0, x, y, derivative);
   blend(encoded, SURFACE, surface_coverage)
}

fn blend(destination: [u8; 3], source: [f32; 4], coverage: f32) -> [u8; 3]
{
   let alpha = source[3] * coverage;
   core::array::from_fn(|channel| {
      let destination = srgb8_to_linear(destination[channel]);
      linear_to_srgb8(source[channel] * alpha + destination * (1.0 - alpha))
   })
}

fn coverage(rect: Rect, radius: f32, x: u32, y: u32, derivative: Derivative) -> f32
{
   let sample_x = x as f32 + 0.5;
   let sample_y = y as f32 + 0.5;
   let signed_distance = distance(rect, radius, sample_x, sample_y);
   let aa = match derivative
   {
      Derivative::QuadFine =>
      {
         let quad_x = (x & !1) as f32;
         let quad_y = (y & !1) as f32;
         let dx = distance(rect, radius, quad_x + 1.5, sample_y)
            - distance(rect, radius, quad_x + 0.5, sample_y);
         let dy = distance(rect, radius, sample_x, quad_y + 1.5)
            - distance(rect, radius, sample_x, quad_y + 0.5);
         dx.abs() + dy.abs()
      }
      Derivative::Central =>
      {
         let dx = distance(rect, radius, sample_x + 0.5, sample_y)
            - distance(rect, radius, sample_x - 0.5, sample_y);
         let dy = distance(rect, radius, sample_x, sample_y + 0.5)
            - distance(rect, radius, sample_x, sample_y - 0.5);
         dx.abs() + dy.abs()
      }
      Derivative::Unit => 1.0,
   }.max(0.0001);
   (0.5 - signed_distance / aa).clamp(0.0, 1.0)
}

fn distance(rect: Rect, radius: f32, x: f32, y: f32) -> f32
{
   let center_x = rect.x + rect.width * 0.5;
   let center_y = rect.y + rect.height * 0.5;
   let q_x = (x - center_x).abs() - (rect.width * 0.5 - radius);
   let q_y = (y - center_y).abs() - (rect.height * 0.5 - radius);
   let outside_x = q_x.max(0.0);
   let outside_y = q_y.max(0.0);
   (outside_x * outside_x + outside_y * outside_y).sqrt() + q_x.max(q_y).min(0.0) - radius
}

fn srgb8_to_linear(value: u8) -> f32
{
   let encoded = f64::from(value) / 255.0;
   if encoded <= 0.04045
   {
      (encoded / 12.92) as f32
   }
   else
   {
      (((encoded + 0.055) / 1.055).powf(2.4)) as f32
   }
}

fn linear_to_srgb8(value: f32) -> u8
{
   let value = f64::from(value.clamp(0.0, 1.0));
   let encoded = if value <= 0.0031308
   {
      value * 12.92
   }
   else
   {
      1.055 * value.powf(1.0 / 2.4) - 0.055
   };
   (encoded * 255.0).round_ties_even().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn background_constants_round_trip_to_the_frozen_srgb_bytes()
   {
      assert_eq!(BACKGROUND.map(linear_to_srgb8), [243, 245, 248]);
   }

   #[test]
   fn quad_derivatives_depend_on_absolute_physical_phase()
   {
      let rect = Rect {x: 0.0, y: 0.0, width: 180.0, height: 180.0};
      let shifted = Rect {x: 1.0, ..rect};
      let differs = (0..36).any(|y| (0..36).any(|x| {
         coverage(rect, 36.0, x, y, Derivative::QuadFine).to_bits()
            != coverage(shifted, 36.0, x + 1, y, Derivative::QuadFine).to_bits()
      }));
      assert!(differs);
   }
}
