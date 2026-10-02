//! Native measurement adapter for the shared comparison fixtures.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::{c_void, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use oxide_comparison_runtime as runtime;
use serde::Serialize;

mod profile;

struct CountingAllocator;
static COUNT_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator
{
   unsafe fn alloc(&self, layout: Layout) -> *mut u8
   {
      if COUNT_ALLOCATIONS.load(Ordering::Relaxed)
      {
         ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
         ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
      }
      unsafe {System.alloc(layout)}
   }

   unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
   {
      unsafe {System.dealloc(ptr, layout)}
   }

   unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8
   {
      if COUNT_ALLOCATIONS.load(Ordering::Relaxed)
      {
         ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
         ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
      }
      unsafe {System.realloc(ptr, layout, size)}
   }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Default, Serialize)]
struct Frame
{
   id: u64,
   source_time: f64,
   scheduled_time: f64,
   target_time: f64,
   elapsed: f64,
   measured: bool,
   admission_wait_ms: f64,
   acquire_ms: f64,
   draw_ms: f64,
   prepare_ms: f64,
   encode_submit_ms: f64,
   presented_time: Option<f64>,
   result: i32,
   gpu_frame_id: u64,
   gpu_ms: f64,
   draws: u32,
   resource_creates: u32,
   resource_grows: u32,
   buffer_upload_bytes: u64,
   texture_upload_bytes: u64,
   preparation_texture_upload_bytes: u64,
   chunks_rebuilt: u64,
   chunks_reused: u64,
   skipped_submissions: u32,
   memory_bytes: u64,
   rust_allocations: u64,
   rust_alloc_bytes: u64,
   #[serde(skip_serializing_if = "Option::is_none")]
   renderer_diagnostics: Option<profile::RendererDiagnostics>,
}

#[derive(Serialize)]
struct HostSample
{
   time: f64,
   cpu_us: u64,
   resident_bytes: u64,
   thermal: i32,
   foreground: bool,
   visible: bool,
   display_awake: bool,
   screen_hz: f64,
   backing_scale: f64,
}

#[derive(Serialize)]
struct OffscreenHostSample
{
   phase: &'static str,
   wall_elapsed: f64,
   cpu_us: u64,
   resident_bytes: u64,
   thermal: i32,
}

#[derive(Serialize)]
struct Run
{
   schema: u32,
   case: String,
   mode: &'static str,
   execution: &'static str,
   display_link: String,
   text_path: &'static str,
   warmup_seconds: f64,
   duration_seconds: f64,
   init_ms: f64,
   measurement_start: f64,
   measurement_end: f64,
   cpu_start_us: u64,
   cpu_end_us: u64,
   external_inputs: u64,
   errors: Vec<String>,
   frames: Vec<Frame>,
   samples: Vec<HostSample>,
   capture_files: Vec<PathBuf>,
   offscreen_host_samples: Vec<OffscreenHostSample>,
   #[serde(skip_serializing_if = "Option::is_none")]
   actual_wall_start_unix_ms: Option<u128>,
   #[serde(skip_serializing_if = "Option::is_none")]
   actual_wall_end_unix_ms: Option<u128>,
   #[serde(skip_serializing_if = "Option::is_none")]
   logical_simulation_seconds: Option<f64>,
   #[serde(skip)]
   start: f64,
   #[serde(skip)]
   next_due: f64,
   #[serde(skip)]
   measured: bool,
   #[serde(skip)]
   generation: u64,
   #[serde(skip)]
   capture_dir: Option<PathBuf>,
   #[serde(skip)]
   output: PathBuf,
}

static RUN: OnceLock<Mutex<Run>> = OnceLock::new();
const CAPTURE_STAGES: [usize; 7] = [0, 1, 2, 0, 1, 2, 0];
const MAX_FRAMES: usize = 16_384;

fn cpu_us() -> u64
{
   let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
   if unsafe {libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr())} != 0 {return 0;}
   let usage = unsafe {usage.assume_init()};
   ((usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as u64) * 1_000_000
      + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as u64
}

fn seconds(name: &str, default: f64) -> f64
{
   std::env::var(name).ok().and_then(|value| value.parse::<f64>().ok())
      .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 60.0).unwrap_or(default)
}

#[no_mangle]
pub extern "C" fn comparison_init(now: f64) -> i32
{
   let name = std::env::var("OXIDE_MAC_CASE").unwrap_or_else(|_| "shapes".to_owned());
   let capture_dir = std::env::var_os("OXIDE_MAC_CAPTURE_DIR").map(PathBuf::from);
   let accounting = std::env::var_os("OXIDE_MAC_ACCOUNTING").is_some();
   let retained_text = matches!(std::env::var("OXIDE_MAC_TEXT_PATH").as_deref(), Ok("retained"));
   runtime::native_set_retained_text_enabled(retained_text);
   let Ok(c_name) = CString::new(name.as_str()) else {return -1;};
   let start = Instant::now();
   let result = unsafe {runtime::oxide_core_suite_init(c_name.as_ptr(), u8::from(capture_dir.is_some()))};
   if result != 0 {eprintln!("comparison suite init failed: {name}: {result}"); return result;}
   runtime::enable_native_measurement(accounting);
   let init_ms = start.elapsed().as_secs_f64() * 1_000.0;
   let now = now + init_ms / 1_000.0;
   let run = Run {
      schema: 1, case: name, mode: if capture_dir.is_some() {"capture"} else if accounting {"accounting"} else {"timing"},
      execution: "native-display",
      display_link: std::env::var("OXIDE_MAC_DISPLAY_LINK").unwrap_or_else(|_| "ca".to_owned()),
      text_path: if retained_text {"retained"} else {"immediate"},
      warmup_seconds: if capture_dir.is_some() {0.0} else {seconds("OXIDE_MAC_WARMUP", 6.0)},
      duration_seconds: if capture_dir.is_some() {7.0} else {seconds("OXIDE_MAC_DURATION", 12.0)},
      init_ms, measurement_start: 0.0, measurement_end: 0.0, cpu_start_us: 0, cpu_end_us: 0,
      external_inputs: 0, errors: Vec::new(), frames: Vec::with_capacity(MAX_FRAMES),
      samples: Vec::with_capacity(128), capture_files: Vec::with_capacity(7),
      offscreen_host_samples: Vec::new(),
      actual_wall_start_unix_ms: None, actual_wall_end_unix_ms: None,
      logical_simulation_seconds: None,
      start: now, next_due: now, measured: false, generation: 0, capture_dir,
      output: std::env::var_os("OXIDE_MAC_OUTPUT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("macos-run.json")),
   };
   if RUN.set(Mutex::new(run)).is_err() {return -1;}
   0
}

#[no_mangle]
pub extern "C" fn comparison_next_wakeup(now: f64) -> f64
{
   let Some(run) = RUN.get() else {return now;};
   let run = run.lock().unwrap();
   let end = run.start + run.warmup_seconds + run.duration_seconds;
   if now >= end {return f64::INFINITY;}
   run.next_due.min(end).min(if run.measured {end} else {run.start + run.warmup_seconds})
}

#[no_mangle]
pub extern "C" fn comparison_end_time() -> f64
{
   let run = RUN.get().unwrap().lock().unwrap();
   run.start + run.warmup_seconds + run.duration_seconds
}

fn scheduled_time(name: &str, origin: f64, elapsed: f64, source: f64, prior_elapsed: Option<f64>) -> f64
{
   if name == "text" || name == "local" {return origin + (elapsed * 10.0).floor() / 10.0;}
   if name.starts_with("visual-") && prior_elapsed.map_or(true, |prior| (prior / 2.0).floor() != (elapsed / 2.0).floor())
   {
      return origin + (elapsed / 2.0).floor() * 2.0;
   }
   source
}

#[no_mangle]
pub extern "C" fn comparison_begin_frame(now: f64, target: f64) -> u64
{
   let Some(run) = RUN.get() else {return 0;};
   let mut run = run.lock().unwrap();
   let origin = run.start + run.warmup_seconds;
   if now >= origin + run.duration_seconds {return 0;}
   if !run.measured && now >= origin
   {
      run.measured = true;
      run.measurement_start = now;
      run.cpu_start_us = cpu_us();
      run.generation = 0;
      run.next_due = now;
      runtime::oxide_core_suite_reset();
   }
   if now + 0.000_001 < run.next_due {return 0;}
   if run.frames.len() == MAX_FRAMES
   {
      run.errors.push("frame collector capacity exceeded".to_owned());
      run.next_due = f64::INFINITY;
      return 0;
   }
   let elapsed = if run.measured {now - origin} else {now - run.start};
   let elapsed = if run.capture_dir.is_some()
   {
      let Some(stage) = CAPTURE_STAGES.get(run.frames.len()) else {return 0;};
      if run.case.starts_with("visual-") {*stage as f64} else {[0.0, 10.0, 19.9][*stage]}
   }
   else {elapsed.max(0.0)};
   let scheduled = scheduled_time(&run.case, if run.measured {origin} else {run.start}, elapsed, now,
      run.frames.last().filter(|frame| frame.measured == run.measured).map(|frame| frame.elapsed));
   let id = run.frames.len() as u64 + 1;
   let measured = run.measured;
   run.frames.push(Frame {id, source_time: now, scheduled_time: scheduled, target_time: target, elapsed, measured, ..Default::default()});
   id
}

#[no_mangle]
pub unsafe extern "C" fn comparison_draw(drawable: *mut c_void, id: u64, acquire_ms: f64) -> i32
{
   let Some(run) = RUN.get() else {return -1;};
   let (elapsed, generation, accounting, capture_path) = {
      let mut run = run.lock().unwrap();
      let Some(frame) = run.frames.get(id.saturating_sub(1) as usize) else {return -1;};
      let elapsed = frame.elapsed;
      let generation = if run.capture_dir.is_some() {(elapsed * 120.0).round() as u64}
         else if run.case == "text" || run.case == "local" {(elapsed * 10.0).floor() as u64 + 1}
         else {run.generation + 1};
      run.generation = generation;
      let capture_path = run.capture_dir.as_ref().map(|path| path.join(format!("{}-{:02}.png", run.case, id)));
      (elapsed, generation, run.mode == "accounting", capture_path)
   };
   let allocations = ALLOCATIONS.load(Ordering::Relaxed);
   let bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
   COUNT_ALLOCATIONS.store(accounting, Ordering::Relaxed);
   let start = Instant::now();
   let result = if drawable.is_null() {-10} else {unsafe {runtime::oxide_core_suite_draw(drawable, elapsed, generation)}};
   let draw_ms = start.elapsed().as_secs_f64() * 1_000.0;
   COUNT_ALLOCATIONS.store(false, Ordering::Relaxed);
   let allocation_count = ALLOCATIONS.load(Ordering::Relaxed) - allocations;
   let allocation_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed) - bytes;
   let metrics = runtime::native_frame_metrics();
   let capture_result = capture_path.as_ref().map(|path| unsafe {runtime::native_capture_drawable(drawable, path)});
   let mut run = run.lock().unwrap();
   let frame = &mut run.frames[id as usize - 1];
   frame.acquire_ms = acquire_ms;
   frame.draw_ms = draw_ms;
   frame.prepare_ms = metrics.prepare_ms;
   frame.encode_submit_ms = metrics.encode_submit_ms;
   frame.result = result;
   let stats = metrics.renderer_perf;
   frame.gpu_frame_id = stats.gpu_frame_id;
   frame.gpu_ms = stats.gpu_ms;
   frame.draws = stats.draws;
   frame.resource_creates = stats.resource_creates;
   frame.resource_grows = stats.resource_grows;
   frame.buffer_upload_bytes = stats.buffer_upload_bytes;
   frame.texture_upload_bytes = stats.texture_upload_bytes;
   frame.preparation_texture_upload_bytes = metrics.preparation_texture_upload_bytes;
   frame.chunks_rebuilt = stats.chunks_rebuilt;
   frame.chunks_reused = stats.chunks_reused;
   frame.memory_bytes = stats.memory.total_bytes;
   frame.rust_allocations = allocation_count;
   frame.rust_alloc_bytes = allocation_bytes;
   if result != 0 {run.errors.push(format!("frame {id}: draw failed {result}"));}
   if let Some(capture_result) = capture_result
   {
      match capture_result
      {
         Ok(()) => run.capture_files.push(capture_path.unwrap()),
         Err(error) => run.errors.push(error),
      }
      run.next_due = run.start + id as f64;
   }
   else
   {
      let origin = if run.measured {run.start + run.warmup_seconds} else {run.start};
      run.next_due = origin + runtime::native_next_wakeup(&run.case, elapsed);
   }
   result
}

#[no_mangle]
pub extern "C" fn comparison_presented(id: u64, time: f64)
{
   if let Some(run) = RUN.get()
   {
      let mut run = run.lock().unwrap();
      if let Some(frame) = run.frames.get_mut(id.saturating_sub(1) as usize)
      {
         if time.is_finite() {frame.presented_time = Some(time);}
      }
   }
}

#[no_mangle]
pub extern "C" fn comparison_host_sample(time: f64, cpu_us: u64, resident_bytes: u64, thermal: i32, foreground: u8, visible: u8, display_awake: u8, screen_hz: f64, backing_scale: f64)
{
   if let Some(run) = RUN.get()
   {
      let mut run = run.lock().unwrap();
      if run.samples.len() < 128
      {
         run.samples.push(HostSample {time, cpu_us, resident_bytes, thermal, foreground: foreground != 0, visible: visible != 0, display_awake: display_awake != 0, screen_hz, backing_scale});
      }
   }
}

#[no_mangle]
pub extern "C" fn comparison_external_input()
{
   if let Some(run) = RUN.get() {run.lock().unwrap().external_inputs += 1;}
}

#[no_mangle]
pub extern "C" fn comparison_display_slept()
{
   if let Some(run) = RUN.get()
   {
      run.lock().unwrap().errors.push("display sleep observed during run".to_owned());
   }
}

#[no_mangle]
pub extern "C" fn comparison_finished(now: f64) -> u8
{
   let Some(run) = RUN.get() else {return 0;};
   let mut run = run.lock().unwrap();
   if now < run.start + run.warmup_seconds + run.duration_seconds {return 0;}
   if run.measurement_end == 0.0
   {
      run.measurement_end = now;
      run.cpu_end_us = cpu_us();
   }
   1
}

#[no_mangle]
pub extern "C" fn comparison_finish(_now: f64)
{
   if let Some(run) = RUN.get()
   {
      let run = run.lock().unwrap();
      if let Some(parent) = run.output.parent() {let _ = std::fs::create_dir_all(parent);}
      match serde_json::to_vec(&*run).map_err(|error| error.to_string())
         .and_then(|bytes| std::fs::write(&run.output, bytes).map_err(|error| error.to_string()))
      {
         Ok(()) => eprintln!("comparison receipt: {}", run.output.display()),
         Err(error) => eprintln!("comparison receipt failed: {error}"),
      }
   }
}

fn unix_millis() -> u128
{
   SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.as_millis())
}

unsafe extern "C"
{
   fn comparison_offscreen_host_sample(resident_bytes: *mut u64, thermal: *mut i32);
}

fn record_offscreen_host_sample(phase: &'static str, wall_start: &Instant)
{
   let mut resident_bytes = 0;
   let mut thermal = 0;
   unsafe {comparison_offscreen_host_sample(&mut resident_bytes, &mut thermal);}
   let mut run = RUN.get().unwrap().lock().unwrap();
   run.offscreen_host_samples.push(OffscreenHostSample {
      phase,
      wall_elapsed: wall_start.elapsed().as_secs_f64(),
      cpu_us: cpu_us(),
      resident_bytes,
      thermal,
   });
}

fn offscreen_record_frame(fixture_elapsed: f64, measured: bool, wall_elapsed: f64, capture_path: Option<PathBuf>)
{
   let (id, generation, accounting) = {
      let mut run = RUN.get().unwrap().lock().unwrap();
      if run.frames.len() == MAX_FRAMES
      {
         run.errors.push("frame collector capacity exceeded".to_owned());
         return;
      }
      let id = run.frames.len() as u64 + 1;
      let generation = if run.capture_dir.is_some()
      {
         (fixture_elapsed * 120.0).round() as u64
      }
      else if run.case == "text" || run.case == "local"
      {
         (fixture_elapsed * 10.0).floor() as u64 + 1
      }
      else
      {
         run.generation + 1
      };
      run.generation = generation;
      run.frames.push(Frame {
         id,
         source_time: wall_elapsed,
         scheduled_time: fixture_elapsed,
         target_time: 0.0,
         elapsed: fixture_elapsed,
         measured,
         ..Default::default()
      });
      (id, generation, run.mode == "accounting")
   };
   let admission_started = Instant::now();
   let admission_result = runtime::native_wait_for_frame_capacity();
   let admission_wait_ms = admission_started.elapsed().as_secs_f64() * 1_000.0;
   if let Err(error) = admission_result
   {
      let mut run = RUN.get().unwrap().lock().unwrap();
      let frame = &mut run.frames[id as usize - 1];
      frame.admission_wait_ms = admission_wait_ms;
      frame.result = -11;
      run.errors.push(format!("frame {id}: capacity admission failed: {error}"));
      return;
   }
   let allocations = ALLOCATIONS.load(Ordering::Relaxed);
   let bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
   COUNT_ALLOCATIONS.store(accounting, Ordering::Relaxed);
   let started = Instant::now();
   let result = runtime::native_draw_offscreen(fixture_elapsed, generation);
   let draw_ms = started.elapsed().as_secs_f64() * 1_000.0;
   COUNT_ALLOCATIONS.store(false, Ordering::Relaxed);
   let allocation_count = ALLOCATIONS.load(Ordering::Relaxed) - allocations;
   let allocation_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed) - bytes;
   let metrics = runtime::native_frame_metrics();
   let capture_result = capture_path.as_ref().map(|path| runtime::native_capture_offscreen(path));
   let mut run = RUN.get().unwrap().lock().unwrap();
   let frame = &mut run.frames[id as usize - 1];
   frame.admission_wait_ms = admission_wait_ms;
   frame.draw_ms = draw_ms;
   frame.prepare_ms = metrics.prepare_ms;
   frame.encode_submit_ms = metrics.encode_submit_ms;
   frame.result = result;
   let stats = metrics.renderer_perf;
   frame.gpu_frame_id = stats.gpu_frame_id;
   frame.gpu_ms = stats.gpu_ms;
   frame.draws = stats.draws;
   frame.resource_creates = stats.resource_creates;
   frame.resource_grows = stats.resource_grows;
   frame.buffer_upload_bytes = stats.buffer_upload_bytes;
   frame.texture_upload_bytes = stats.texture_upload_bytes;
   frame.preparation_texture_upload_bytes = metrics.preparation_texture_upload_bytes;
   frame.chunks_rebuilt = stats.chunks_rebuilt;
   frame.chunks_reused = stats.chunks_reused;
   frame.skipped_submissions = stats.skipped_submissions;
   frame.memory_bytes = stats.memory.total_bytes;
   frame.rust_allocations = allocation_count;
   frame.rust_alloc_bytes = allocation_bytes;
   if accounting {frame.renderer_diagnostics = Some(profile::RendererDiagnostics::from_metrics(metrics));}
   if result != 0
   {
      run.errors.push(format!("frame {id}: draw failed {result}"));
   }
   if stats.skipped_submissions != 0
   {
      run.errors.push(format!("frame {id}: renderer skipped {} submissions", stats.skipped_submissions));
   }
   if let Some(capture_result) = capture_result
   {
      match capture_result
      {
         Ok(()) => run.capture_files.push(capture_path.unwrap()),
         Err(error) => run.errors.push(error),
      }
   }
}

fn run_offscreen() -> i32
{
   let name = std::env::var("OXIDE_MAC_CASE").unwrap_or_else(|_| "shapes".to_owned());
   let capture_dir = std::env::var_os("OXIDE_MAC_CAPTURE_DIR").map(PathBuf::from);
   let accounting = std::env::var_os("OXIDE_MAC_ACCOUNTING").is_some();
   let background_accounting = accounting && capture_dir.is_none();
   let retained_text = matches!(std::env::var("OXIDE_MAC_TEXT_PATH").as_deref(), Ok("retained"));
   let Ok(c_name) = CString::new(name.as_str()) else {return -1;};
   runtime::native_set_retained_text_enabled(retained_text);
   let initialization = Instant::now();
   let result = unsafe {runtime::oxide_core_suite_init(c_name.as_ptr(), u8::from(capture_dir.is_some()))};
   if result != 0
   {
      eprintln!("comparison suite init failed: {name}: {result}");
      return result;
   }
   runtime::enable_native_measurement(accounting);
   let init_ms = initialization.elapsed().as_secs_f64() * 1_000.0;
   let wall_start_unix_ms = unix_millis();
   let wall_start = Instant::now();
   let mode = if capture_dir.is_some() {"capture"} else if accounting {"accounting"} else {"timing"};
   let warmup_seconds = if capture_dir.is_some() {0.0} else {6.0};
   let duration_seconds = if capture_dir.is_some() {7.0} else {12.0};
   let run = Run {
      schema: 1,
      case: name.clone(),
      mode,
      execution: "offscreen-replay",
      display_link: "offscreen".to_owned(),
      text_path: if retained_text {"retained"} else {"immediate"},
      warmup_seconds,
      duration_seconds,
      init_ms,
      measurement_start: 0.0,
      measurement_end: 0.0,
      cpu_start_us: 0,
      cpu_end_us: 0,
      external_inputs: 0,
      errors: Vec::new(),
      frames: Vec::with_capacity(MAX_FRAMES),
      samples: Vec::new(),
      capture_files: Vec::with_capacity(7),
      offscreen_host_samples: Vec::with_capacity(3),
      actual_wall_start_unix_ms: Some(wall_start_unix_ms),
      actual_wall_end_unix_ms: None,
      logical_simulation_seconds: Some(warmup_seconds + duration_seconds),
      start: 0.0,
      next_due: 0.0,
      measured: false,
      generation: 0,
      capture_dir: capture_dir.clone(),
      output: std::env::var_os("OXIDE_MAC_OUTPUT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("macos-run.json")),
   };
   if RUN.set(Mutex::new(run)).is_err()
   {
      return -1;
   }
   if background_accounting
   {
      record_offscreen_host_sample("warmup-start", &wall_start);
   }

   if let Some(directory) = capture_dir
   {
      for (index, stage) in CAPTURE_STAGES.into_iter().enumerate()
      {
         let fixture_elapsed = if name.starts_with("visual-") {stage as f64} else {[0.0, 10.0, 19.9][stage]};
         let path = directory.join(format!("{}-{:02}.png", name, index + 1));
         offscreen_record_frame(fixture_elapsed, false, wall_start.elapsed().as_secs_f64(), Some(path));
      }
   }
   else
   {
      let warmup_ticks = 6 * 120;
      let total_ticks = (6 + 12) * 120;
      let mut next_due_tick = 0u64;
      for tick in 0..total_ticks
      {
         let measured = tick >= warmup_ticks;
         if tick == warmup_ticks
         {
            if let Err(error) = runtime::native_wait_for_gpu()
            {
               RUN.get().unwrap().lock().unwrap().errors.push(format!("warmup GPU drain failed: {error}"));
               break;
            }
            runtime::oxide_core_suite_reset();
            let mut run = RUN.get().unwrap().lock().unwrap();
            run.measured = true;
            run.measurement_start = wall_start.elapsed().as_secs_f64();
            run.cpu_start_us = cpu_us();
            run.generation = 0;
            next_due_tick = tick;
            drop(run);
            if background_accounting
            {
               record_offscreen_host_sample("measurement-start", &wall_start);
            }
         }
         if tick < next_due_tick
         {
            continue;
         }
         let fixture_elapsed = if measured {(tick - warmup_ticks) as f64 / 120.0} else {tick as f64 / 120.0};
         offscreen_record_frame(fixture_elapsed, measured, wall_start.elapsed().as_secs_f64(), None);
         let next_due = runtime::native_next_wakeup(&name, fixture_elapsed);
         if !next_due.is_finite()
         {
            RUN.get().unwrap().lock().unwrap().errors.push("fixture returned an invalid offscreen wakeup".to_owned());
            break;
         }
         let next_tick = (next_due * 120.0).round() as u64;
         next_due_tick = if measured {warmup_ticks + next_tick} else {next_tick};
      }
      let mut run = RUN.get().unwrap().lock().unwrap();
      run.measurement_end = wall_start.elapsed().as_secs_f64();
      run.cpu_end_us = cpu_us();
      drop(run);
      if let Err(error) = runtime::native_wait_for_gpu()
      {
         RUN.get().unwrap().lock().unwrap().errors.push(format!("measurement GPU drain failed: {error}"));
      }
      if background_accounting
      {
         record_offscreen_host_sample("measurement-end", &wall_start);
      }
   }
   let mut run = RUN.get().unwrap().lock().unwrap();
   run.actual_wall_end_unix_ms = Some(unix_millis());
   if run.measurement_end == 0.0
   {
      run.measurement_end = wall_start.elapsed().as_secs_f64();
      run.cpu_end_us = cpu_us();
   }
   drop(run);
   comparison_finish(0.0);
   0
}

fn main()
{
   if std::env::var_os("OXIDE_MAC_OFFSCREEN").is_some()
   {
      if std::env::var_os("OXIDE_MAC_PROFILE_SECONDS").is_some()
      {
         std::process::exit(profile::run());
      }
      std::process::exit(run_offscreen());
   }
   #[cfg(target_os = "macos")]
   {
      extern "C" {fn comparison_main() -> i32;}
      std::process::exit(unsafe {comparison_main()});
   }
   #[cfg(not(target_os = "macos"))]
   panic!("the native comparison adapter requires macOS");
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn deadlines_separate_action_latency_from_continuous_frames()
   {
      assert_eq!(scheduled_time("text", 100.0, 0.319, 100.319, Some(0.21)), 100.3);
      assert_eq!(scheduled_time("visual-layout", 100.0, 2.01, 102.01, Some(0.0)), 102.0);
      assert_eq!(scheduled_time("visual-controls", 100.0, 2.02, 102.02, Some(2.01)), 102.02);
      assert_eq!(scheduled_time("scroll", 100.0, 2.02, 102.02, Some(2.01)), 102.02);
   }
}
