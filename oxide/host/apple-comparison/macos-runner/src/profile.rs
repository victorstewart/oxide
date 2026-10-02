//! Bounded, allocation-light replay for external CPU/allocation profiling.
//! Diagnostic throughput only: no frame-pacing or presentation measurements.

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Instant;

use oxide_comparison_runtime as runtime;
use serde::Serialize;

unsafe extern "C"
{
   fn comparison_profile_interval(begin: u8);
}

macro_rules! diagnostic_counters
{
   ($($field:ident),+ $(,)?) => {
      #[derive(Serialize)]
      pub(super) struct RendererDiagnostics
      {
         $($field: u64,)+
      }

      impl RendererDiagnostics
      {
         pub(super) fn from_metrics(metrics: runtime::NativeFrameMetrics) -> Self
         {
            let stats = metrics.renderer_perf;
            Self {$($field: stats.$field as u64,)+}
         }
      }
   };
}

diagnostic_counters!(
   commands_traversed, commands_copied, geometry_bytes_copied,
   chunks_prepared, prepared_plan_reuses, backend_cache_hits, backend_cache_misses,
   property_upload_bytes, property_records_updated, property_ring_bytes,
   layer_body_commands_scanned, layer_body_commands_copied,
   layer_texture_creates, layer_cache_hits, layer_cache_misses,
   layer_cache_resident_bytes, layer_cache_pool_bytes, layer_cache_cpu_bytes,
   layer_cache_pool_reuses, layer_cache_evictions, layer_cache_recreations,
   layer_offscreen_draws, layer_inline_draws,
   image_argument_encodes, image_argument_binds, image_argument_tables_finalized,
   image_argument_table_reuses, image_argument_bytes,
   render_passes, blit_passes, command_buffers,
   texture_copies, texture_copy_pixels, texture_copy_bytes,
   glyph_instances, glyph_instance_bytes, glyph_instance_buffer_binds,
   damage_instances_visited, damage_commands_visited, damage_vertices_visited,
   shaded_damage_px, damage_px, damage_rects, damage_forced_full_refreshes,
   persistent_target_valid, culled, actual_submissions,
);

#[derive(Default, Serialize)]
struct Profile
{
   schema: u32,
   execution: &'static str,
   case: String,
   timeline: &'static str,
   requested_wall_seconds: f64,
   requested_cycles: Option<u64>,
   init_ms: f64,
   warmup_frames: u64,
   measured_frames: u64,
   complete_cycles: u64,
   measured_wall_seconds: f64,
   measured_cpu_us: u64,
   started_unix_ms: u128,
   profile_started_unix_ms: u128,
   profile_ended_unix_ms: u128,
   errors: Vec<String>,
}

fn replay(name: &str, ticks: u64, offset: f64, deadline: Option<&Instant>, seconds: f64) -> Result<(u64, bool), String>
{
   let mut frames = 0;
   let mut tick = 0;
   while tick < ticks
   {
      if deadline.is_some_and(|start| start.elapsed().as_secs_f64() >= seconds) {return Ok((frames, false));}
      runtime::native_wait_for_frame_capacity()?;
      let elapsed = offset + tick as f64 / 120.0;
      let result = runtime::native_draw_offscreen(elapsed, (elapsed * 120.0).round() as u64 + 1);
      if result != 0 {return Err(format!("draw returned {result} at tick {tick}"));}
      if runtime::native_frame_metrics().renderer_perf.skipped_submissions != 0
      {
         return Err(format!("skipped submission at tick {tick}"));
      }
      frames += 1;
      let next = runtime::native_next_wakeup(name, elapsed);
      if !next.is_finite() || next < elapsed {return Err(format!("invalid next wakeup {next}"));}
      tick = (((next - offset) * 120.0).round() as u64).max(tick + 1);
   }
   Ok((frames, true))
}

pub(super) fn run() -> i32
{
   let name = std::env::var("OXIDE_MAC_CASE").unwrap_or_else(|_| "shapes".to_owned());
   let seconds = super::seconds("OXIDE_MAC_PROFILE_SECONDS", 12.0);
   let continuous = std::env::var_os("OXIDE_MAC_PROFILE_CONTINUOUS").is_some();
   let cycles = match std::env::var("OXIDE_MAC_BENCH_CYCLES")
   {
      Ok(value) => match value.parse::<u64>()
      {
         Ok(value) if value > 0 && value <= 1_000 => Some(value),
         _ => {eprintln!("benchmark cycles must be in 1..=1000"); return 2;}
      },
      Err(_) => None,
   };
   if seconds <= 0.0 || !runtime::NATIVE_CASES.contains(&name.as_str()) {return 2;}
   let mut receipt = Profile {
      schema: 1, execution: if cycles.is_some() {"offscreen-fixed-replay"} else {"offscreen-profile"},
      case: name.clone(), requested_wall_seconds: seconds, requested_cycles: cycles,
      timeline: if continuous {"continuous-text-stress"} else {"repeated-cached-cycle"},
      started_unix_ms: super::unix_millis(), ..Default::default()
   };
   runtime::native_set_retained_text_enabled(false);
   let c_name = CString::new(name.as_str()).unwrap();
   let initialized = Instant::now();
   let result = unsafe {runtime::oxide_core_suite_init(c_name.as_ptr(), 0)};
   if result != 0 {eprintln!("profile init failed: {result}"); return 1;}
   runtime::enable_native_measurement(false);
   receipt.init_ms = initialized.elapsed().as_secs_f64() * 1_000.0;
   match replay(&name, 6 * 120, 0.0, None, seconds).and_then(|(frames, _)| runtime::native_wait_for_gpu().map(|_| frames))
   {
      Ok(frames) => receipt.warmup_frames = frames,
      Err(error) => receipt.errors.push(error),
   }
   let started = Instant::now();
   let cpu_started = super::cpu_us();
   receipt.profile_started_unix_ms = super::unix_millis();
   unsafe {comparison_profile_interval(1);}
   while receipt.errors.is_empty() && cycles.map_or_else(|| started.elapsed().as_secs_f64() < seconds, |limit| receipt.complete_cycles < limit)
   {
      if !continuous {runtime::oxide_core_suite_reset();}
      let offset = if continuous {receipt.complete_cycles as f64 * 12.0} else {0.0};
      match replay(&name, 12 * 120, offset, if cycles.is_some() {None} else {Some(&started)}, seconds)
      {
         Ok((frames, complete)) => {
            receipt.measured_frames += frames;
            receipt.complete_cycles += u64::from(complete);
         }
         Err(error) => receipt.errors.push(error),
      }
   }
   if cycles.is_some()
   {
      if let Err(error) = runtime::native_wait_for_gpu() {receipt.errors.push(error);}
   }
   unsafe {comparison_profile_interval(0);}
   receipt.profile_ended_unix_ms = super::unix_millis();
   receipt.measured_cpu_us = super::cpu_us() - cpu_started;
   receipt.measured_wall_seconds = started.elapsed().as_secs_f64();
   if let Err(error) = runtime::native_wait_for_gpu() {receipt.errors.push(error);}
   let output = std::env::var_os("OXIDE_MAC_OUTPUT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("macos-profile.json"));
   if let Some(parent) = output.parent() {if let Err(error) = std::fs::create_dir_all(parent) {eprintln!("{error}"); return 1;}}
   let result = serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())
      .and_then(|bytes| std::fs::write(output, bytes).map_err(|error| error.to_string()));
   if let Err(error) = result {eprintln!("profile receipt failed: {error}"); return 1;}
   let hold = super::seconds("OXIDE_MAC_PROFILE_HOLD_SECONDS", 0.0);
   if hold > 0.0 {std::thread::sleep(std::time::Duration::from_secs_f64(hold));}
   if receipt.errors.is_empty() {0} else {1}
}
