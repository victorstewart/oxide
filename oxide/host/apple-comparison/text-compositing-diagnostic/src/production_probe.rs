use std::{
   fs,
   path::Path,
};

use oxide_renderer_api as api;
use oxide_text::Font;
use oxide_ui_core::{elements, DrawListBuilder};
use serde::Serialize;

use super::{sha256, DiagnosticRequest, TitleCase};

const TEXT_LINEAR: api::Color = api::Color::rgba(0.014443844, 0.017641954, 0.025186860, 1.0);

#[derive(Serialize)]
struct MetalProbeManifest
{
   schema_version: u32,
   case_id: String,
   canvas_width: u32,
   canvas_height: u32,
   viewport_width_bits: u32,
   viewport_height_bits: u32,
   atlas_path: String,
   atlas_width: u32,
   atlas_height: u32,
   atlas_row_bytes: usize,
   atlas_sha256: String,
   instances: Vec<MetalGlyphInstance>,
}

#[derive(Serialize)]
struct MetalGlyphInstance
{
   dst_bits: [u32; 4],
   uv_bits: [u32; 4],
   color_bits: [u32; 4],
}

struct CaptureUploader
{
   next_handle: u32,
   atlas: Option<CapturedAtlas>,
   error: Option<String>,
}

struct CapturedAtlas
{
   handle: api::ImageHandle,
   width: u32,
   height: u32,
   data: Vec<u8>,
}

impl CaptureUploader
{
   fn new() -> Self
   {
      Self {next_handle: 1, atlas: None, error: None}
   }
}

impl elements::ImageUploader for CaptureUploader
{
   fn create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
   {
      let handle = api::ImageHandle(self.next_handle);
      self.next_handle = self.next_handle.wrapping_add(1).max(1);
      let Some(length) = (width as usize).checked_mul(height as usize) else
      {
         self.error = Some("production atlas dimensions overflow".to_string());
         return handle;
      };
      let mut tight = vec![0_u8; length];
      if row_bytes < width as usize || data.len() < row_bytes.saturating_mul(height as usize)
      {
         self.error = Some("production atlas upload is incomplete".to_string());
         return handle;
      }
      for row in 0..height as usize
      {
         let source = row * row_bytes;
         let destination = row * width as usize;
         tight[destination..destination + width as usize]
            .copy_from_slice(&data[source..source + width as usize]);
      }
      self.atlas = Some(CapturedAtlas {handle, width, height, data: tight});
      handle
   }

   fn update_a8(&mut self, handle: api::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      let Some(atlas) = self.atlas.as_mut() else
      {
         self.error = Some("production atlas update preceded creation".to_string());
         return;
      };
      if atlas.handle != handle || x.saturating_add(width) > atlas.width || y.saturating_add(height) > atlas.height
         || row_bytes < width as usize || data.len() < row_bytes.saturating_mul(height as usize)
      {
         self.error = Some("production atlas update is outside the captured page".to_string());
         return;
      }
      for row in 0..height as usize
      {
         let source = row * row_bytes;
         let destination = (y as usize + row) * atlas.width as usize + x as usize;
         atlas.data[destination..destination + width as usize]
            .copy_from_slice(&data[source..source + width as usize]);
      }
   }
}

pub(super) fn prepare(request: &DiagnosticRequest, font_bytes: &[u8], output_directory: &Path) -> Result<(), String>
{
   for case in &request.cases
   {
      prepare_case(request, case, font_bytes, output_directory)?;
   }
   Ok(())
}

fn prepare_case(request: &DiagnosticRequest, case: &TitleCase, font_bytes: &[u8], output_directory: &Path) -> Result<(), String>
{
   let font = Font::from_bytes(font_bytes.to_vec());
   let frame = case.frame_millionths.map(|value| value as f32 / 1_000_000.0);
   let point_size = request.point_size_millionths as f32 / 1_000_000.0;
   let baseline = ((frame[1] + (frame[3] - point_size * 1.362) * 0.5 + point_size * 1.069)
      * request.device_scale as f32).round() / request.device_scale as f32;
   let mut text = elements::TextCtx::default();
   let font_id = text.fonts.add_font(font);
   text.begin_frame();
   let mut uploader = CaptureUploader::new();
   let mut builder = DrawListBuilder::new();
   let align = match case.alignment
   {
      "left" => elements::Align::Left,
      "center" => elements::Align::Center,
      _ => return Err(format!("unsupported production alignment {}", case.alignment)),
   };
   elements::encode_label_text(
      case.text,
      TEXT_LINEAR,
      align,
      false,
      font_id,
      point_size,
      api::RectF::new(frame[0], baseline, frame[2], frame[3]),
      request.device_scale as f32,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   let _ = text.finish_frame(&mut uploader, &mut builder);
   if let Some(error) = uploader.error
   {
      return Err(error);
   }
   let atlas = uploader.atlas.ok_or_else(|| "production title did not publish an A8 atlas".to_string())?;
   let list = builder.into_inner();
   let mut instances = Vec::new();
   for item in &list.items
   {
      let api::DrawCmd::GlyphRun {run} = item else {continue};
      let start = run.vb.offset as usize;
      let end = start.saturating_add(run.vb.len as usize);
      let vertices = list.vertices.get(start..end).ok_or_else(|| "production glyph span is invalid".to_string())?;
      if run.atlas != atlas.handle || run.sdf || vertices.len() % 4 != 0
      {
         return Err("production title did not lower to canonical A8 quads".to_string());
      }
      for quad in vertices.chunks_exact(4)
      {
         let [top_left, top_right, bottom_left, bottom_right] = quad else {continue};
         if top_left.y != top_right.y || bottom_left.y != bottom_right.y
            || top_left.x != bottom_left.x || top_right.x != bottom_right.x
            || top_left.v != top_right.v || bottom_left.v != bottom_right.v
            || top_left.u != bottom_left.u || top_right.u != bottom_right.u
         {
            return Err("production glyph quad is not axis aligned".to_string());
         }
         instances.push(MetalGlyphInstance {
            dst_bits: [top_left.x.to_bits(), top_left.y.to_bits(), (top_right.x - top_left.x).to_bits(), (bottom_left.y - top_left.y).to_bits()],
            uv_bits: [top_left.u.to_bits(), top_left.v.to_bits(), top_right.u.to_bits(), bottom_left.v.to_bits()],
            color_bits: [run.color.r.to_bits(), run.color.g.to_bits(), run.color.b.to_bits(), run.color.a.to_bits()],
         });
      }
   }
   if instances.is_empty()
   {
      return Err("production title emitted no glyph instances".to_string());
   }
   let atlas_name = format!("{}.production-atlas.a8", case.id);
   fs::write(output_directory.join(&atlas_name), &atlas.data)
      .map_err(|error| format!("writing production atlas: {error}"))?;
   let manifest = MetalProbeManifest {
      schema_version: 1,
      case_id: case.id.to_string(),
      canvas_width: request.canvas_width,
      canvas_height: request.canvas_height,
      viewport_width_bits: (request.canvas_width as f32 / request.device_scale as f32).to_bits(),
      viewport_height_bits: (request.canvas_height as f32 / request.device_scale as f32).to_bits(),
      atlas_path: atlas_name,
      atlas_width: atlas.width,
      atlas_height: atlas.height,
      atlas_row_bytes: atlas.width as usize,
      atlas_sha256: sha256(&atlas.data),
      instances,
   };
   let manifest = serde_json::to_vec_pretty(&manifest)
      .map_err(|error| format!("encoding production Metal manifest: {error}"))?;
   fs::write(output_directory.join(format!("{}.production-probe.json", case.id)), manifest)
      .map_err(|error| format!("writing production Metal manifest: {error}"))?;
   Ok(())
}
