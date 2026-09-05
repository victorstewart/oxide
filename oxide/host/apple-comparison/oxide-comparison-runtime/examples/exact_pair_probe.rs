//! Sparse pixel attribution for comparison-only exact PNG pairs.

use std::{env, fs::File, path::PathBuf, process::ExitCode};

use serde_json::json;

struct Image
{
   width: u32,
   height: u32,
   rgba: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Rect
{
   x: u32,
   y: u32,
   width: u32,
   height: u32,
}

impl Image
{
   fn pixel(&self, x: u32, y: u32) -> [u8; 4]
   {
      let offset = ((y * self.width + x) * 4) as usize;
      self.rgba[offset..offset + 4].try_into().unwrap()
   }
}

fn main() -> ExitCode
{
   match run()
   {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) =>
      {
         eprintln!("exact pair probe failed: {error}");
         ExitCode::FAILURE
      }
   }
}

fn run() -> Result<(), String>
{
   let (oxide_path, native_path, requested_rect) = arguments()?;
   let oxide = decode_png(&oxide_path)?;
   let native = decode_png(&native_path)?;
   if oxide.width != native.width || oxide.height != native.height
   {
      return Err(format!("image dimensions differ: Oxide is {}x{}, native is {}x{}", oxide.width, oxide.height, native.width, native.height));
   }
   let rect = requested_rect.unwrap_or(Rect {x: 0, y: 0, width: oxide.width, height: oxide.height});
   if rect.x.saturating_add(rect.width) > oxide.width || rect.y.saturating_add(rect.height) > oxide.height
   {
      return Err(String::from("comparison rectangle exceeds the image bounds"));
   }
   let mut points = Vec::new();
   let mut channels = 0_u64;
   let mut maximum = 0_u8;
   for y in rect.y..rect.y + rect.height
   {
      for x in rect.x..rect.x + rect.width
      {
         let oxide_pixel = oxide.pixel(x, y);
         let native_pixel = native.pixel(x, y);
         let delta = core::array::from_fn::<_, 3, _>(|channel| oxide_pixel[channel].abs_diff(native_pixel[channel]));
         let pixel_maximum = *delta.iter().max().unwrap();
         if pixel_maximum == 0 {continue}
         channels += delta.iter().filter(|value| **value != 0).count() as u64;
         maximum = maximum.max(pixel_maximum);
         points.push(json!({
            "x": x,
            "y": y,
            "oxide": &oxide_pixel[..3],
            "native": &native_pixel[..3],
            "delta": delta,
            "maximum": pixel_maximum,
         }));
      }
   }
   let shift_trials = (-3..=3).flat_map(|dy| (-3..=3).map(move |dx| (dx, dy))).map(|(dx, dy)|
   {
      let mut pixels = 0_u64;
      let mut channels = 0_u64;
      let mut total_delta = 0_u64;
      let mut maximum = 0_u8;
      for y in rect.y..rect.y + rect.height
      {
         for x in rect.x..rect.x + rect.width
         {
            let native_x = x as i32 + dx;
            let native_y = y as i32 + dy;
            if native_x < 0 || native_y < 0 || native_x >= native.width as i32 || native_y >= native.height as i32 {continue}
            let oxide_pixel = oxide.pixel(x, y);
            let native_pixel = native.pixel(native_x as u32, native_y as u32);
            let mut pixel_differs = false;
            for channel in 0..3
            {
               let delta = oxide_pixel[channel].abs_diff(native_pixel[channel]);
               pixel_differs |= delta != 0;
               channels += u64::from(delta != 0);
               total_delta += u64::from(delta);
               maximum = maximum.max(delta);
            }
            pixels += u64::from(pixel_differs);
         }
      }
      json!({"native_dx": dx, "native_dy": dy, "differing_pixels": pixels, "differing_channels": channels, "total_channel_delta": total_delta, "maximum_channel_delta": maximum})
   }).collect::<Vec<_>>();
   println!("{}", json!({
      "schema_version": 1,
      "algorithm": "exact-pair-probe-v1",
      "oxide_png": oxide_path,
      "native_png": native_path,
      "width": oxide.width,
      "height": oxide.height,
      "rect": {"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height},
      "differing_pixels": points.len(),
      "differing_channels": channels,
      "maximum_channel_delta": maximum,
      "points": points,
      "shift_trials": shift_trials,
   }));
   Ok(())
}

fn arguments() -> Result<(PathBuf, PathBuf, Option<Rect>), String>
{
   let mut arguments = env::args().skip(1);
   let mut oxide = None;
   let mut native = None;
   let mut rect = None;
   while let Some(argument) = arguments.next()
   {
      match argument.as_str()
      {
         "--oxide" => oxide = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--native" => native = Some(PathBuf::from(arguments.next().ok_or_else(usage)?)),
         "--rect" => rect = Some(parse_rect(&arguments.next().ok_or_else(usage)?)?),
         "--help" | "-h" => return Err(usage()),
         _ => return Err(format!("unknown argument {argument}\n{}", usage())),
      }
   }
   Ok((oxide.ok_or_else(usage)?, native.ok_or_else(usage)?, rect))
}

fn usage() -> String
{
   String::from("usage: exact_pair_probe --oxide OXIDE.png --native NATIVE.png [--rect X,Y,WIDTH,HEIGHT]")
}

fn parse_rect(value: &str) -> Result<Rect, String>
{
   let values = value.split(',').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>()
      .map_err(|_| format!("invalid comparison rectangle {value}"))?;
   if values.len() != 4 || values[2] == 0 || values[3] == 0
   {
      return Err(format!("invalid comparison rectangle {value}"));
   }
   Ok(Rect {x: values[0], y: values[1], width: values[2], height: values[3]})
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
