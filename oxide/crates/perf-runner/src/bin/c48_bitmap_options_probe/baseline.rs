use oxide_renderer_api::{
   Color, GlyphRun, ImageHandle, Insets, RectF, RectI, RenderEncoder, Vertex,
};
use oxide_ui_core::{
   draw_text_input_options_popover, text_input_options_layout, TextInputOptionsConfig,
   TextInputOptionsPopoverStyle,
};
use oxide_wasm_alloc_counter::{snapshot, CountingAllocator};
use serde::Serialize;
use std::alloc::System;
use std::hint::black_box;
use std::time::{Duration, Instant};

#[path = "common.rs"]
mod metal_probe;

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

const SAMPLE_COUNT: usize = 31;
const WARMUP_SAMPLE_COUNT: usize = 5;
const OPS_PER_SAMPLE: usize = 2_048;
const CONDITIONING_TIME: Duration = Duration::from_millis(100);

#[derive(Default)]
struct CountingEncoder
{
   solids: u64,
   rrects: u64,
   glyph_runs: u64,
}

impl CountingEncoder
{
   fn clear(&mut self)
   {
      self.solids = 0;
      self.rrects = 0;
      self.glyph_runs = 0;
   }

   fn commands(&self) -> u64
   {
      self.solids + self.rrects + self.glyph_runs
   }
}

impl RenderEncoder for CountingEncoder
{
   fn set_viewport(&mut self, _vp: RectF) {}

   fn set_clip(&mut self, _scissor: RectI) {}

   fn draw_solid(&mut self, _verts: &[Vertex], _color: Color)
   {
      self.solids = self.solids.wrapping_add(1);
   }

   fn draw_image(&mut self, _img: ImageHandle, _dst: RectF, _src: RectF) {}

   fn draw_rrect(&mut self, _rect: RectF, _radii: [f32; 4], _color: Color)
   {
      self.rrects = self.rrects.wrapping_add(1);
   }

   fn draw_nine_slice(&mut self, _img: ImageHandle, _rect: RectF, _slice: Insets, _alpha: f32) {}

   fn draw_backdrop(&mut self, _rect: RectF, _sigma: f32, _tint: Color, _alpha: f32) {}

   fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

   fn draw_glyph_run(&mut self, _run: &GlyphRun)
   {
      self.glyph_runs = self.glyph_runs.wrapping_add(1);
   }
}

#[derive(Serialize)]
struct Report
{
   variant: &'static str,
   warmup_samples_us: Vec<f64>,
   samples_us: Vec<f64>,
   operations_per_sample: usize,
   commands_per_op: u64,
   solid_draws_per_op: u64,
   label_solid_draws_per_op: u64,
   glyph_runs_per_op: u64,
   render_mutex_locks_per_op: u64,
   allocs_per_op: f64,
   alloc_bytes_per_op: f64,
   reallocs_per_op: f64,
   checksum: u64,
}

fn main()
{
   if std::env::var_os("OXIDE_C48_METAL").is_some()
   {
      run_metal();
      return;
   }
   let layout = text_input_options_layout(
      RectF::new(260.0, 80.0, 120.0, 44.0),
      RectF::new(0.0, 0.0, 640.0, 480.0),
      1.0,
      TextInputOptionsConfig::all(),
      10.6,
   )
   .expect("option layout");
   let style = TextInputOptionsPopoverStyle {
      background: Color::rgba(0.01, 0.01, 0.01, 0.96),
      divider: Color::rgba(1.0, 1.0, 1.0, 0.78),
      text: Color::rgba(1.0, 1.0, 1.0, 0.96),
      text_px: 10.6,
   };
   let mut encoder = CountingEncoder::default();
   let conditioning_started = Instant::now();
   while conditioning_started.elapsed() < CONDITIONING_TIME
   {
      encoder.clear();
      draw_text_input_options_popover(&mut encoder, layout, style);
      black_box(encoder.commands());
   }
   let mut warmup_samples_us = Vec::with_capacity(WARMUP_SAMPLE_COUNT);
   for _ in 0..WARMUP_SAMPLE_COUNT
   {
      let started = Instant::now();
      for _ in 0..OPS_PER_SAMPLE
      {
         encoder.clear();
         draw_text_input_options_popover(&mut encoder, layout, style);
         black_box(encoder.commands());
      }
      warmup_samples_us.push(
         started.elapsed().as_secs_f64() * 1_000_000.0 / OPS_PER_SAMPLE as f64,
      );
   }
   let commands_per_op = encoder.commands();
   let solid_draws_per_op = encoder.solids;
   let label_solid_draws_per_op = solid_draws_per_op.saturating_sub(2);
   let glyph_runs_per_op = encoder.glyph_runs;

   let alloc_before = snapshot();
   for _ in 0..OPS_PER_SAMPLE
   {
      encoder.clear();
      draw_text_input_options_popover(&mut encoder, layout, style);
      black_box(encoder.commands());
   }
   let alloc_after = snapshot();

   let mut samples_us = Vec::with_capacity(SAMPLE_COUNT);
   let mut checksum = 0_u64;
   for _ in 0..SAMPLE_COUNT
   {
      let started = Instant::now();
      for _ in 0..OPS_PER_SAMPLE
      {
         encoder.clear();
         draw_text_input_options_popover(&mut encoder, layout, style);
         checksum = checksum.wrapping_add(black_box(encoder.commands()));
      }
      samples_us.push(started.elapsed().as_secs_f64() * 1_000_000.0 / OPS_PER_SAMPLE as f64);
   }

   let operations = OPS_PER_SAMPLE as f64;
   let report = Report {
      variant: "parent-per-alpha-run",
      warmup_samples_us,
      samples_us,
      operations_per_sample: OPS_PER_SAMPLE,
      commands_per_op,
      solid_draws_per_op,
      label_solid_draws_per_op,
      glyph_runs_per_op,
      render_mutex_locks_per_op: 4,
      allocs_per_op: (alloc_after.alloc_count - alloc_before.alloc_count) as f64 / operations,
      alloc_bytes_per_op: (alloc_after.alloc_bytes - alloc_before.alloc_bytes) as f64 / operations,
      reallocs_per_op: (alloc_after.realloc_count - alloc_before.realloc_count) as f64 / operations,
      checksum,
   };
   write_report(&report);
}

fn run_metal()
{
   let style = TextInputOptionsPopoverStyle {
      background: Color::rgba(0.01, 0.01, 0.01, 0.96),
      divider: Color::rgba(1.0, 1.0, 1.0, 0.78),
      text: Color::rgba(1.0, 1.0, 1.0, 0.96),
      text_px: 10.6,
   };
   let mut encoder = metal_probe::DrawListEncoder::default();
   let mut popovers = 0;
   for row in 0..4
   {
      for column in 0..2
      {
         let layout = text_input_options_layout(
            RectF::new(
               100.0 + column as f32 * 320.0,
               80.0 + row as f32 * 100.0,
               120.0,
               44.0,
            ),
            RectF::new(0.0, 0.0, 640.0, 480.0),
            1.0,
            TextInputOptionsConfig::all(),
            10.6,
         )
         .expect("option layout");
         draw_text_input_options_popover(&mut encoder, layout, style);
         popovers += 1;
      }
   }
   metal_probe::measure_metal("parent-per-alpha-run", encoder.into_inner(), None, popovers);
}

fn write_report(report: &Report)
{
   let json = serde_json::to_string(report).expect("serialize report");
   if let Some(path) = std::env::args_os().nth(1)
   {
      std::fs::write(path, format!("{json}\n")).expect("write report");
   }
   else
   {
      println!("{json}");
   }
}
