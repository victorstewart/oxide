//! Comparison-only Rustybuzz position probe for CoreText parity diagnosis.

use std::{env, fs, process::ExitCode};

use oxide_text::{Font, TextShaper};
use serde_json::json;

fn main() -> ExitCode
{
   match run()
   {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) =>
      {
         eprintln!("shape position probe failed: {error}");
         ExitCode::FAILURE
      }
   }
}

fn run() -> Result<(), String>
{
   let mut arguments = env::args().skip(1);
   let font_path = arguments.next().ok_or_else(usage)?;
   let text = arguments.next().ok_or_else(usage)?;
   let point_size = arguments.next().ok_or_else(usage)?.parse::<f32>().map_err(|_| usage())?;
   if arguments.next().is_some() || !point_size.is_finite() || point_size <= 0.0
   {
      return Err(usage());
   }
   let font = Font::from_bytes(fs::read(&font_path).map_err(|error| format!("reading {font_path}: {error}"))?);
   let mut shaper = TextShaper::default();
   let shape = shaper.shape(&font, 0, &text, point_size).map_err(|error| error.to_string())?;
   let boundaries = (0..=text.len()).collect::<Vec<_>>();
   let positions = shape.prefix_widths_for_boundaries(&boundaries);
   println!("{}", json!({
      "schema_version": 1,
      "algorithm": "rustybuzz-shape-position-probe-v1",
      "font": font_path,
      "text": text,
      "point_size": point_size,
      "width": shape.width(),
      "positions": positions,
   }));
   Ok(())
}

fn usage() -> String
{
   String::from("usage: shape_position_probe FONT.ttf TEXT POINT_SIZE")
}
