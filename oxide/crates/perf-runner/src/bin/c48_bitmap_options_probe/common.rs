use oxide_renderer_api::{
   Color, DrawCmd, DrawList, FrameTarget, GlyphRun, ImageHandle, IndexSpan, Insets, RectF,
   RectI, RenderEncoder, Renderer, Vertex, VertexSpan,
};
use oxide_renderer_metal::MetalRenderer;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const SCALE: f32 = 2.0;
const WARMUP_FRAMES: usize = 16;
const MEASURED_FRAMES: usize = 140;

pub struct AtlasImage<'a>
{
   pub data: &'a [u8],
   pub width: u32,
   pub height: u32,
}

#[derive(Default)]
pub struct DrawListEncoder
{
   list: DrawList,
}

impl DrawListEncoder
{
   pub fn into_inner(self) -> DrawList
   {
      self.list
   }

   fn append_vertices(&mut self, vertices: &[Vertex]) -> Option<VertexSpan>
   {
      let offset = u32::try_from(self.list.vertices.len()).ok()?;
      let len = u32::try_from(vertices.len()).ok()?;
      self.list.vertices.extend_from_slice(vertices);
      Some(VertexSpan { offset, len })
   }

   fn append_indices(&mut self, indices: &[u16]) -> Option<IndexSpan>
   {
      let offset = u32::try_from(self.list.indices.len()).ok()?;
      let len = u32::try_from(indices.len()).ok()?;
      self.list.indices.extend_from_slice(indices);
      Some(IndexSpan { offset, len })
   }
}

impl RenderEncoder for DrawListEncoder
{
   fn set_viewport(&mut self, _vp: RectF) {}

   fn set_clip(&mut self, scissor: RectI)
   {
      self.list.items.push(DrawCmd::ClipPush { rect: scissor });
   }

   fn draw_solid(&mut self, vertices: &[Vertex], color: Color)
   {
      let Some(vb) = self.append_vertices(vertices) else {
         return;
      };
      let ib = IndexSpan { offset: 0, len: 0 };
      self.list.items.push(DrawCmd::Solid { vb, ib, color });
   }

   fn draw_image(&mut self, tex: ImageHandle, dst: RectF, src: RectF)
   {
      self.list.items.push(DrawCmd::Image { tex, dst, src, alpha: 1.0 });
   }

   fn draw_image_mesh(
      &mut self,
      tex: ImageHandle,
      vertices: &[Vertex],
      indices: &[u16],
      alpha: f32,
   )
   {
      let Some(vb) = self.append_vertices(vertices) else {
         return;
      };
      let Some(ib) = self.append_indices(indices) else {
         return;
      };
      self.list.items.push(DrawCmd::ImageMesh { tex, vb, ib, alpha });
   }

   fn draw_rrect(&mut self, rect: RectF, radii: [f32; 4], color: Color)
   {
      self.list.items.push(DrawCmd::RRect { rect, radii, color });
   }

   fn draw_nine_slice(&mut self, tex: ImageHandle, rect: RectF, slice: Insets, alpha: f32)
   {
      self.list.items.push(DrawCmd::NineSlice { tex, rect, slice, alpha });
   }

   fn draw_backdrop(&mut self, rect: RectF, sigma: f32, tint: Color, alpha: f32)
   {
      self.list.items.push(DrawCmd::Backdrop { rect, sigma, tint, alpha });
   }

   fn draw_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32)
   {
      self.list.items.push(DrawCmd::Spinner { center, atom, alpha });
   }

   fn draw_glyph_run(&mut self, run: &GlyphRun)
   {
      self.list.items.push(DrawCmd::GlyphRun { run: *run });
   }

   fn draw_glyph_run_resolved(
      &mut self,
      run: &GlyphRun,
      vertices: &[Vertex],
      indices: &[u16],
   )
   {
      let Some(vb) = self.append_vertices(vertices) else {
         return;
      };
      let Some(ib) = self.append_indices(indices) else {
         return;
      };
      self.list.items.push(DrawCmd::GlyphRun { run: GlyphRun { vb, ib, ..*run } });
   }
}

#[derive(Serialize)]
struct MetalReport
{
   variant: &'static str,
   warmup_gpu_ms: Vec<f64>,
   gpu_ms: Vec<f64>,
   frame_submit_ms: Vec<f64>,
   encode_ms: Vec<f64>,
   commands: usize,
   draws: u32,
   popovers_per_frame: usize,
   pixel_sha256: String,
   completed_timestamps: usize,
}

pub fn measure_metal(
   variant: &'static str,
   list: DrawList,
   atlas: Option<AtlasImage<'_>>,
   popovers_per_frame: usize,
)
{
   let mut renderer = MetalRenderer::new_default().expect("create C48 Metal renderer");
   renderer.resize(WIDTH, HEIGHT, SCALE).expect("resize C48 Metal renderer");
   let atlas_handle = match atlas {
      Some(atlas) => renderer.image_create_a8(atlas.width, atlas.height, atlas.data, atlas.width as usize),
      None => renderer.image_create_a8(1, 1, &[0], 1),
   };
   assert_eq!(atlas_handle, ImageHandle(1), "C48 atlas handle identity");

   let mut warmup_gpu_ms = Vec::with_capacity(WARMUP_FRAMES);
   let mut gpu_ms = Vec::with_capacity(MEASURED_FRAMES);
   let mut frame_submit_ms = Vec::with_capacity(MEASURED_FRAMES);
   let mut encode_ms = Vec::with_capacity(MEASURED_FRAMES);
   let mut draws = 0;
   let mut completed_timestamps = 0;
   for frame in 0..WARMUP_FRAMES + MEASURED_FRAMES
   {
      let started = Instant::now();
      let token = renderer.begin_frame(&FrameTarget, None);
      renderer.encode_pass(&list);
      renderer.submit(token).expect("submit C48 Metal frame");
      let submit_ms = started.elapsed().as_secs_f64() * 1_000.0;
      let stats = completed_stats(&renderer, token.0);
      if stats.gpu_frame_id == token.0 && stats.gpu_ms > 0.0
      {
         completed_timestamps += 1;
      }
      if frame < WARMUP_FRAMES
      {
         warmup_gpu_ms.push(stats.gpu_ms);
         continue;
      }
      gpu_ms.push(stats.gpu_ms);
      frame_submit_ms.push(submit_ms);
      encode_ms.push(stats.encode_ms);
      draws = stats.draws;
   }
   assert_eq!(completed_timestamps, WARMUP_FRAMES + MEASURED_FRAMES, "completed C48 GPU timestamps");
   let (_, _, pixels) = renderer.readback_bgra8().expect("read C48 Metal pixels");
   let report = MetalReport {
      variant,
      warmup_gpu_ms,
      gpu_ms,
      frame_submit_ms,
      encode_ms,
      commands: list.items.len(),
      draws,
      popovers_per_frame,
      pixel_sha256: format!("{:x}", Sha256::digest(&pixels)),
      completed_timestamps,
   };
   write_report(&report);
}

fn completed_stats(renderer: &MetalRenderer, frame_id: u64) -> oxide_renderer_metal::PerfStats
{
   let mut stats = renderer.last_stats();
   for _ in 0..100
   {
      if stats.gpu_frame_id == frame_id
      {
         break;
      }
      std::thread::sleep(Duration::from_millis(1));
      stats = renderer.last_stats();
   }
   stats
}

fn write_report(report: &MetalReport)
{
   let json = serde_json::to_string(report).expect("serialize C48 Metal report");
   if let Some(path) = std::env::args_os().nth(1)
   {
      std::fs::write(path, format!("{json}\n")).expect("write C48 Metal report");
   }
   else
   {
      println!("{json}");
   }
}
