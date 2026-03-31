//! Oxide Metal renderer (metal-rs backend)
#![allow(clippy::all, clippy::pedantic)]
#![allow(unexpected_cfgs)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::unnecessary_cast,
    clippy::borrow_as_ptr,
    clippy::items_after_statements,
    useless_ptr_null_checks,
    clippy::bool_to_int_with_if,
    clippy::nonminimal_bool,
    clippy::too_many_lines,
    clippy::explicit_iter_loop,
    clippy::unnecessary_get_then_check,
    clippy::map_unwrap_or,
    clippy::ref_as_ptr,
    clippy::match_same_arms,
    clippy::implicit_clone,
    clippy::semicolon_if_nothing_returned,
    clippy::unnecessary_min_or_max,
    clippy::too_many_arguments,
    clippy::missing_safety_doc,
    clippy::uninlined_format_args,
    clippy::manual_let_else,
    clippy::ptr_as_ptr,
    clippy::needless_borrow,
    clippy::unnecessary_wraps,
    clippy::must_use_candidate,
    clippy::similar_names,
    unused_variables
)]

use block::ConcreteBlock;
use core::f32::consts::TAU;
use core::ptr::NonNull;
use metal::foreign_types::ForeignType;
use metal::foreign_types::ForeignTypeRef;
use metal::{self, *};
use objc::msg_send;
use objc::runtime::Object;
use objc::sel;
use objc::sel_impl;
use oxide_renderer_api as api;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CStr;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{atomic::AtomicBool, Arc, OnceLock};
use std::time::Instant;
use thiserror::Error;

#[cfg(target_os = "ios")]
extern "C" {
    fn oxide_host_ios_log(ptr: *const core::ffi::c_char, len: usize);
    fn oxide_host_perf_signpost_begin(ptr: *const core::ffi::c_char, len: usize) -> u64;
    fn oxide_host_perf_signpost_end(ptr: *const core::ffi::c_char, len: usize, signpost_id: u64);
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn ios_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let oxide = std::env::var("OXIDE_RUST_LOG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let nametag = std::env::var("NAMETAG_DEBUG_RUNTIME_CREATE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        oxide || nametag
    })
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn ios_log(msg: &str) {
    #[cfg(target_os = "ios")]
    unsafe {
        if ios_log_enabled() {
            oxide_host_ios_log(msg.as_ptr() as *const core::ffi::c_char, msg.len());
        }
    }
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn camera_perf_trace_signposts_enabled() -> bool {
    #[cfg(target_os = "ios")]
    {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("OXIDE_PERF_CAMERA_TRACE_PHASES")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        })
    }
    #[cfg(not(target_os = "ios"))]
    {
        false
    }
}

#[inline(always)]
fn camera_perf_stage_stats_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("OXIDE_PERF_PARKED")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

#[inline(always)]
fn experimental_tiny_camera_preview_renderer_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

#[inline(always)]
fn experimental_preview_submission_backpressure_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

#[inline(always)]
pub fn direct_preview_submission_backpressure_applies(enabled: bool, in_flight: usize) -> bool {
    enabled && in_flight >= 2
}

#[inline(always)]
#[cfg(target_os = "ios")]
fn ios_monotonic_now_ns() -> u64 {
    static TIMEBASE: OnceLock<(u64, u64)> = OnceLock::new();
    let (numer, denom) = *TIMEBASE.get_or_init(|| {
        let mut info = mach2::mach_time::mach_timebase_info_data_t { numer: 0, denom: 0 };
        let status = unsafe { mach2::mach_time::mach_timebase_info(&mut info) };
        if status != mach2::kern_return::KERN_SUCCESS || info.denom == 0 {
            return (0, 1);
        }
        (u64::from(info.numer), u64::from(info.denom))
    });
    if numer == 0 {
        return 0;
    }
    let ticks = unsafe { mach2::mach_time::mach_absolute_time() };
    ticks.saturating_mul(numer) / denom.max(1)
}

#[inline(always)]
#[cfg(not(target_os = "ios"))]
fn ios_monotonic_now_ns() -> u64 {
    0
}

#[inline(always)]
fn direct_preview_present_frame_age_ms(timestamp_ns: u64) -> f64 {
    if timestamp_ns == 0 {
        return 0.0;
    }
    let now_ns = ios_monotonic_now_ns();
    if now_ns <= timestamp_ns {
        return 0.0;
    }
    (now_ns - timestamp_ns) as f64 / 1_000_000.0
}

#[inline(always)]
fn elapsed_ms(start: Option<Instant>) -> f64 {
    start.map(|value| value.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
}

#[cfg(target_os = "ios")]
#[inline(always)]
unsafe fn command_buffer_gpu_duration_ms(buffer: &CommandBufferRef) -> f64 {
    let gpu_start_time: f64 = msg_send![buffer.as_ptr(), GPUStartTime];
    let gpu_end_time: f64 = msg_send![buffer.as_ptr(), GPUEndTime];
    if gpu_start_time.is_finite()
        && gpu_end_time.is_finite()
        && gpu_start_time > 0.0
        && gpu_end_time >= gpu_start_time
    {
        return (gpu_end_time - gpu_start_time) * 1000.0;
    }
    0.0
}

#[cfg(not(target_os = "ios"))]
#[inline(always)]
unsafe fn command_buffer_gpu_duration_ms(_buffer: &CommandBufferRef) -> f64 {
    0.0
}

#[inline(always)]
fn with_perf_signpost<T>(_name: &str, body: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "ios")]
    {
        if !camera_perf_trace_signposts_enabled() {
            return body();
        }
        let signpost_id = unsafe {
            oxide_host_perf_signpost_begin(_name.as_ptr() as *const core::ffi::c_char, _name.len())
        };
        let result = body();
        unsafe {
            oxide_host_perf_signpost_end(
                _name.as_ptr() as *const core::ffi::c_char,
                _name.len(),
                signpost_id,
            );
        }
        return result;
    }
    #[cfg(not(target_os = "ios"))]
    {
        body()
    }
}

#[inline(always)]
fn nsstring_to_string(ns: *mut Object) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    unsafe {
        let c: *const std::os::raw::c_char = msg_send![ns, UTF8String];
        if c.is_null() {
            None
        } else {
            Some(CStr::from_ptr(c).to_string_lossy().into_owned())
        }
    }
}

#[inline(always)]
fn env_flag(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|value| {
        matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    })
}

#[inline(always)]
fn camera_render_mode_from_env() -> Option<CameraRenderMode> {
    std::env::var("OXIDE_CAMERA_RENDER_MODE").ok().and_then(|value| {
        match value.trim().to_ascii_lowercase().as_str() {
            "nv12_optimized" | "optimized" | "default" => Some(CameraRenderMode::Nv12Optimized),
            "nv12_legacy" | "legacy" => Some(CameraRenderMode::Nv12Legacy),
            "bgra_benchmark" | "bgra" => Some(CameraRenderMode::BgraBenchmark),
            _ => None,
        }
    })
}

#[inline(always)]
fn camera_texture_source_from_env() -> Option<CameraTextureSource> {
    std::env::var("OXIDE_CAMERA_TEXTURE_SOURCE").ok().and_then(|value| {
        match value.trim().to_ascii_lowercase().as_str() {
            "live" | "camera" => Some(CameraTextureSource::Live),
            "synthetic" | "benchmark" | "synthetic_benchmark" => {
                Some(CameraTextureSource::SyntheticBenchmark)
            }
            _ => None,
        }
    })
}

static EXTERNAL_MTL_DEVICE_PTR: AtomicUsize = AtomicUsize::new(0);

pub fn set_external_mtl_device_ptr(device_ptr: *mut core::ffi::c_void) {
    let old = EXTERNAL_MTL_DEVICE_PTR.swap(device_ptr as usize, Ordering::AcqRel) as *mut MTLDevice;
    if !old.is_null() {
        unsafe {
            // The host passes retained Objective-C pointers. If a newer pointer
            // overwrites an older one before consumption, release the stale retain.
            drop(Device::from_ptr(old));
        }
    }
}

fn take_external_mtl_device() -> Option<Device> {
    let raw = EXTERNAL_MTL_DEVICE_PTR.swap(0, Ordering::AcqRel) as *mut MTLDevice;
    if raw.is_null() {
        return None;
    }
    ios_log("oxide.renderer-metal: init using external MTLDevice pointer");
    Some(unsafe { Device::from_ptr(raw) })
}

#[inline(always)]
fn encode_debug_stride() -> usize {
    std::env::var("NAMETAG_DEBUG_ENCODE_EVERY")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

#[inline(always)]
fn draw_cmd_kind(cmd: &api::DrawCmd) -> &'static str {
    match cmd {
        api::DrawCmd::LayerBegin { .. } => "layer_begin",
        api::DrawCmd::LayerEnd => "layer_end",
        api::DrawCmd::Solid { .. } => "solid",
        api::DrawCmd::Image { .. } => "image",
        api::DrawCmd::GlyphRun { .. } => "glyph_run",
        api::DrawCmd::RRect { .. } => "rrect",
        api::DrawCmd::NineSlice { .. } => "nine_slice",
        api::DrawCmd::Backdrop { .. } => "backdrop",
        api::DrawCmd::CameraBg { .. } => "camera_bg",
        api::DrawCmd::Spinner { .. } => "spinner",
        api::DrawCmd::ClipPush { .. } => "clip_push",
        api::DrawCmd::ClipPop => "clip_pop",
    }
}

#[inline(always)]
fn running_on_ios_simulator() -> bool {
    cfg!(target_os = "ios")
        && (cfg!(target_abi = "sim") || std::env::var_os("SIMULATOR_UDID").is_some())
}

#[inline(always)]
fn apply_simulator_safety_bool(simulator: bool, enabled: bool) -> bool {
    if simulator {
        return false;
    }
    enabled
}

#[inline(always)]
fn apply_simulator_sample_count(simulator: bool, sample_count: u32) -> u32 {
    if simulator {
        return 1;
    }
    sample_count.max(1)
}

#[inline(always)]
fn apply_simulator_hdr(simulator: bool, wants_hdr: bool) -> bool {
    if simulator {
        return false;
    }
    wants_hdr
}

#[inline(always)]
fn glyph_icb_enabled_default() -> bool {
    // iOS Simulator has shown unstable glyph command execution with ICB in parity runs.
    // Prefer deterministic direct draws there unless explicitly re-enabled.
    if running_on_ios_simulator() {
        return false;
    }
    // The current glyph ICB path is not yet production-safe on device either.
    // Keep the default on the direct draw path until the ICB pipeline is fixed.
    false
}

#[inline(always)]
fn layer_cache_enabled_default() -> bool {
    // Layer texture caching has exhibited stale/blank composition on Simulator.
    // Prefer deterministic inline layer rendering there unless explicitly enabled.
    if running_on_ios_simulator() {
        return false;
    }
    true
}

#[inline(always)]
fn glyph_icb_resource_options() -> MTLResourceOptions {
    // ICB recording calls `indirect_render_command_at_index`, which is CPU access.
    // Private storage faults under Metal validation and can surface as submit errors.
    MTLResourceOptions::StorageModeShared
}

// Metal `set*Bytes` APIs are limited to 4 KiB payloads per call.
// Keep instanced parameter uploads under this limit by chunking draws.
const METAL_SET_BYTES_LIMIT: usize = 4096;
const FRAME_RING_SIZE: usize = 8;
const IMAGE_ARG_TEXTURE_SLOTS: u32 = 128;
const LEGACY_SPINNER_LARGE_ATOM: f32 = 37.0;
const LEGACY_SPINNER_LARGE_STROKE: f32 = 2.5;
const LEGACY_SPINNER_ROTATION_MS: u64 = 1_000;

#[inline]
fn legacy_spinner_thickness(atom: f32) -> f32 {
    (atom.max(1.0) / LEGACY_SPINNER_LARGE_ATOM * LEGACY_SPINNER_LARGE_STROKE).max(1.0)
}

#[inline]
fn legacy_spinner_radius(atom: f32) -> f32 {
    let clamped_atom = atom.max(1.0);
    (clamped_atom * 0.5 - legacy_spinner_thickness(clamped_atom)).max(2.0)
}

#[inline]
fn legacy_spinner_phase(now_ms: u64) -> f32 {
    let progress = (now_ms % LEGACY_SPINNER_ROTATION_MS) as f32 / LEGACY_SPINNER_ROTATION_MS as f32;
    progress * TAU
}

#[inline]
fn spinner_now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[inline(always)]
fn max_instances_per_set_bytes(v_bytes_per_instance: usize, f_bytes_per_instance: usize) -> usize {
    let v = if v_bytes_per_instance == 0 {
        usize::MAX
    } else {
        METAL_SET_BYTES_LIMIT / v_bytes_per_instance
    };
    let f = if f_bytes_per_instance == 0 {
        usize::MAX
    } else {
        METAL_SET_BYTES_LIMIT / f_bytes_per_instance
    };
    v.min(f).max(1)
}

#[derive(Debug, Error)]
pub enum MetalInitError {
    #[error("no metal device available")]
    NoDevice,
    #[error("failed to create command queue")]
    NoQueue,
    #[error("failed to compile shader library: {0}")]
    Library(String),
    #[error("pipeline state error in {0}")]
    Pipeline(String),
}

#[inline(always)]
fn pipeline_error(stage: &str, message: impl Into<String>) -> MetalInitError {
    let message = message.into();
    eprintln!("[Oxide] renderer pipeline failure stage={stage}: {message}");
    ios_log(&format!("oxide.renderer-metal: pipeline failure stage={} message={}", stage, message));
    MetalInitError::Pipeline(format!("{}: {}", stage, message))
}

#[inline(always)]
fn pipeline_function(lib: &Library, stage: &str, name: &str) -> Result<Function, MetalInitError> {
    lib.get_function(name, None).map_err(|err| pipeline_error(stage, err))
}

#[inline(always)]
fn pipeline_state(
    device: &Device,
    stage: &str,
    desc: &RenderPipelineDescriptor,
) -> Result<RenderPipelineState, MetalInitError> {
    desc.set_label(stage);
    device.new_render_pipeline_state(desc).map_err(|err| pipeline_error(stage, err))
}

#[inline(always)]
fn build_init_stage<T>(
    stage: &'static str,
    build: impl FnOnce() -> Result<T, MetalInitError>,
) -> Result<T, MetalInitError> {
    ios_log(&format!("oxide.renderer-metal: init building {}", stage));
    let result = build();
    if let Err(err) = &result {
        eprintln!("[Oxide] renderer init failed stage={stage}: {err}");
        ios_log(&format!("oxide.renderer-metal: init failed stage={} err={}", stage, err));
    }
    result
}

#[inline(always)]
fn pipeline_mentions_indirect_command_buffers(err: &MetalInitError) -> bool {
    match err {
        MetalInitError::Pipeline(message) => {
            message.to_ascii_lowercase().contains("indirect command buffers")
        }
        _ => false,
    }
}

const SHADERS_SRC: &str = concat!(
    include_str!("../shaders/solid.metal"),
    "\n",
    include_str!("../shaders/effects.metal"),
    "\n",
    include_str!("../shaders/ui.metal"),
    "\n",
    include_str!("../shaders/text.metal"),
    "\n",
    include_str!("../shaders/camera.metal"),
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetalRendererConfig {
    pub wants_hdr: bool,
    pub sample_count: u32,
    pub camera_render_mode: CameraRenderMode,
    pub camera_texture_source: CameraTextureSource,
    pub direct_preview_only: bool,
}

impl Default for MetalRendererConfig {
    fn default() -> Self {
        Self {
            wants_hdr: false,
            sample_count: 1,
            camera_render_mode: camera_render_mode_from_env()
                .unwrap_or(CameraRenderMode::Nv12Optimized),
            camera_texture_source: camera_texture_source_from_env()
                .unwrap_or(CameraTextureSource::Live),
            direct_preview_only: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraRenderMode {
    Nv12Optimized,
    Nv12Legacy,
    BgraBenchmark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraTextureSource {
    Live,
    SyntheticBenchmark,
}

#[allow(dead_code)]
pub struct MetalRenderer {
    device: Device,
    queue: CommandQueue,
    pso_solid: RenderPipelineState,
    pso_image: RenderPipelineState,
    pso_image_single: RenderPipelineState,
    pso_blur: RenderPipelineState,
    pso_downsample: RenderPipelineState,
    pso_upsample: RenderPipelineState,
    pso_backdrop: RenderPipelineState,
    pso_rrect: RenderPipelineState,
    pso_nine_slice: RenderPipelineState,
    pso_spinner: RenderPipelineState,
    pso_text: RenderPipelineState,
    pso_text_sdf: RenderPipelineState,
    pso_camera: RenderPipelineState,
    pso_camera_legacy: RenderPipelineState,
    pso_camera_preview_fast_full: RenderPipelineState,
    pso_camera_preview_fast_video: RenderPipelineState,
    pso_camera_bgra: RenderPipelineState,
    // Argument buffer for image textures
    img_arg: Option<ArgumentEncoder>,
    img_arg_buf: Option<Buffer>,
    sampler: Option<SamplerState>,
    color_format: MTLPixelFormat,
    config: MetalRendererConfig,
    sample_count: u32,
    hdr_enabled: bool,
    frame_id: u64,
    frames: [PerFrame; FRAME_RING_SIZE],
    vb: Ring,
    ib: Ring,
    ub: Ring,
    target_w: u32,
    target_h: u32,
    target_scale: f32,
    target_tex: Option<Texture>,
    target_msaa_tex: Option<Texture>,
    prepass_tex: Option<Texture>,
    blur_tmp_tex: Option<Texture>,
    half_tex: Option<Texture>,
    quarter_tex: Option<Texture>,
    quarter_tmp_tex: Option<Texture>,
    images: HashMap<u32, Texture>,
    next_image_id: u32,
    layers: HashMap<u32, LayerEntry>,
    layer_cache_enabled: bool,
    last_stats: PerfStats,
    acc_draws: u32,
    acc_instanced: u32,
    acc_icb_cmds: u32,
    use_glyph_icb: bool,
    // Damage rendering flag and per-frame scissor (dp) if provided
    damage_enabled: bool,
    frame_scissor_dp: Option<api::RectI>,
    frame_damage_rects: u32,
    frame_damage_pct: f32,
    frame_damage_px: u64,
    acc_culled: u32,
    damage_use_thresh: f32,
    damage_prefilter_thresh: f32,
    main_shaded_px: u64,
    prepass_shaded_px: u64,
    scissor_changes: u32,
    // Camera blur cache + scheduling
    cam_blur_tex: Option<Texture>,
    cam_last_update: Option<std::time::Instant>,
    cam_update_period: std::time::Duration,
    // Adaptive/pause state
    cam_paused: bool,
    cam_pause_frames: u32,
    // Camera props and transitions
    last_cam_w: i32,
    last_cam_h: i32,
    last_cam_bd: i32,
    last_cam_mx: i32,
    last_cam_vr: i32,
    last_cam_cs: i32,
    last_cam_fetch_ms: f64,
    cam_xfade_prev_tex: Option<Texture>,
    cam_xfade_t0: Option<std::time::Instant>,
    cam_xfade_ms: u32,
    cam_blur_fade_t0: Option<std::time::Instant>,
    camera_render_mode: CameraRenderMode,
    camera_texture_source: CameraTextureSource,
    current_live_camera_frame: Option<LiveCameraNv12Frame>,
    camera_preview_renderer: Option<CameraPreviewRenderer>,
    bench_cam_y_tex: Option<Texture>,
    bench_cam_uv_tex: Option<Texture>,
    bench_cam_bgra_tex: Option<Texture>,
    use_camera_textures: bool,
    use_image_arg_buffer: bool,
    submit_error_flag: Arc<AtomicBool>,
    direct_preview_submitted: VecDeque<(u64, CommandBuffer)>,
    direct_preview_last_submission_depth: u32,
    direct_preview_last_submission_skipped: u32,
    direct_preview_last_present_frame_age_ms: f64,
    direct_preview_last_completed_frame_id: u64,
    direct_preview_last_completed_gpu_ms: f64,
    pending_present_drawable: usize,
    pending_present_texture: usize,
}

#[allow(dead_code)]
struct CameraPreviewRenderer {
    queue: CommandQueue,
    pso_camera: RenderPipelineState,
    pso_camera_legacy: RenderPipelineState,
    pso_camera_preview_fast_full: RenderPipelineState,
    pso_camera_preview_fast_video: RenderPipelineState,
    submit_error_flag: Arc<AtomicBool>,
    inflight_submissions: Arc<AtomicUsize>,
}

#[derive(Clone, Copy, Debug, Default)]
struct CameraPreviewRenderResult {
    drew_live_frame: bool,
    camera_width: i32,
    camera_height: i32,
    camera_bit_depth: i32,
    camera_matrix: i32,
    camera_video_range: i32,
    camera_color_space: i32,
    setup_ms: f64,
    encode_quad_ms: f64,
    present_ms: f64,
    commit_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct DirectCameraEncodeStats {
    camera_width: i32,
    camera_height: i32,
    camera_bit_depth: i32,
    camera_matrix: i32,
    camera_video_range: i32,
    camera_color_space: i32,
    bind_ms: f64,
    draw_ms: f64,
}

impl CameraPreviewRenderer {
    fn new(
        queue: CommandQueue,
        pso_camera: RenderPipelineState,
        pso_camera_legacy: RenderPipelineState,
        pso_camera_preview_fast_full: RenderPipelineState,
        pso_camera_preview_fast_video: RenderPipelineState,
    ) -> Self {
        Self {
            queue,
            pso_camera,
            pso_camera_legacy,
            pso_camera_preview_fast_full,
            pso_camera_preview_fast_video,
            submit_error_flag: Arc::new(AtomicBool::new(false)),
            inflight_submissions: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn submit_error_pending(&self) -> bool {
        self.submit_error_flag.load(Ordering::Acquire)
    }

    fn take_submit_error(&self) -> bool {
        self.submit_error_flag.swap(false, Ordering::AcqRel)
    }

    fn pending_submission_count(&self) -> u32 {
        self.inflight_submissions.load(Ordering::Acquire) as u32
    }

    fn camera_pipeline_for_frame(
        &self,
        frame: &LiveCameraNv12Frame,
        mode: CameraRenderMode,
    ) -> &RenderPipelineState {
        if direct_preview_uses_fast_yuv_pipeline(frame.bit_depth, frame.matrix, frame.video_range) {
            if frame.video_range == 0 {
                return &self.pso_camera_preview_fast_full;
            }
            if frame.video_range == 1 {
                return &self.pso_camera_preview_fast_video;
            }
        }
        match mode {
            CameraRenderMode::Nv12Legacy => &self.pso_camera_legacy,
            CameraRenderMode::Nv12Optimized | CameraRenderMode::BgraBenchmark => &self.pso_camera,
        }
    }

    unsafe fn render_live_frame(
        &mut self,
        drawable_ptr: *mut core::ffi::c_void,
        frame: Option<&LiveCameraNv12Frame>,
        width: u32,
        height: u32,
        scale: f32,
        mode: CameraRenderMode,
        collect_stage_stats: bool,
    ) -> Result<CameraPreviewRenderResult, api::RenderError> {
        let mut result = CameraPreviewRenderResult::default();
        let Some(frame) = frame else {
            return Ok(result);
        };
        if drawable_ptr.is_null() {
            return Ok(result);
        }

        let vp_dp = [(width as f32) / scale.max(1.0), (height as f32) / scale.max(1.0)];
        let rect_dp = [0.0, 0.0, vp_dp[0], vp_dp[1]];
        let command_buffer_t0 = collect_stage_stats.then(Instant::now);
        let cmd = self.queue.new_command_buffer().to_owned();
        let command_buffer_ms = elapsed_ms(command_buffer_t0);
        let rpd = RenderPassDescriptor::new();
        let setup_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost("camera.renderer.direct.setup", || -> Result<(), api::RenderError> {
            let raw_drawable_obj = drawable_ptr as *mut Object;
            let raw_dst_tex: *mut MTLTexture = msg_send![raw_drawable_obj, texture];
            if raw_dst_tex.is_null() {
                return Err(api::RenderError::InvalidOperation(
                    "drawable did not provide a destination texture",
                ));
            }
            let ca0 = rpd.color_attachments().object_at(0).unwrap();
            ca0.set_texture(Some(TextureRef::from_ptr(raw_dst_tex)));
            ca0.set_store_action(MTLStoreAction::Store);
            ca0.set_load_action(MTLLoadAction::Clear);
            ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
            Ok(())
        })?;
        result.setup_ms = elapsed_ms(setup_t0);

        let encoder_t0 = collect_stage_stats.then(Instant::now);
        let enc = cmd.new_render_command_encoder(&rpd);
        let encoder_ms = elapsed_ms(encoder_t0);
        let encode_quad_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost("camera.renderer.direct.encode_quad", || {
            let (uv_scale, uv_bias) =
                camera_aspect_fill_params(rect_dp[2], rect_dp[3], frame.width, frame.height);
            let params = pack_camera_params(
                rect_dp,
                api::Color::rgba(1.0, 1.0, 1.0, 1.0),
                1.0,
                uv_scale,
                uv_bias,
                false,
                frame.matrix,
                frame.video_range,
                frame.bit_depth,
            );
            let y_tex = TextureRef::from_ptr(frame.y_tex as *mut MTLTexture);
            let uv_tex = TextureRef::from_ptr(frame.uv_tex as *mut MTLTexture);
            enc.set_render_pipeline_state(self.camera_pipeline_for_frame(frame, mode));
            enc.set_fragment_texture(0, Some(y_tex));
            enc.set_fragment_texture(1, Some(uv_tex));
            enc.set_fragment_bytes(
                1,
                core::mem::size_of_val(&params) as u64,
                (&params as *const CameraGpuParams).cast(),
            );
            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
        });
        result.encode_quad_ms = elapsed_ms(encode_quad_t0);
        enc.end_encoding();

        let raw_drawable = drawable_ptr as *mut MTLDrawable;
        let drawable = DrawableRef::from_ptr(raw_drawable);
        let present_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost(
            "camera.renderer.direct.present_drawable",
            || -> Result<(), api::RenderError> {
                cmd.present_drawable(drawable);
                Ok(())
            },
        )?;
        result.present_ms = elapsed_ms(present_t0);

        let submit_error_flag = Arc::clone(&self.submit_error_flag);
        let inflight_submissions = Arc::clone(&self.inflight_submissions);
        inflight_submissions.fetch_add(1, Ordering::AcqRel);
        let completion = ConcreteBlock::new(move |buffer: &CommandBufferRef| {
            if buffer.status() == MTLCommandBufferStatus::Error {
                submit_error_flag.store(true, Ordering::Release);
            }
            inflight_submissions.fetch_sub(1, Ordering::AcqRel);
        })
        .copy();
        cmd.add_completed_handler(&completion);

        let commit_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost("camera.renderer.direct.commit", || {
            cmd.commit();
        });
        result.commit_ms = elapsed_ms(commit_t0);
        result.drew_live_frame = true;
        result.camera_width = frame.width;
        result.camera_height = frame.height;
        result.camera_bit_depth = frame.bit_depth;
        result.camera_matrix = frame.matrix;
        result.camera_video_range = frame.video_range;
        result.camera_color_space = frame.color_space;
        Ok(result)
    }
}

impl MetalRenderer {
    fn new_with_config_impl(config: MetalRendererConfig) -> Result<Self, MetalInitError> {
        let simulator = running_on_ios_simulator();
        ios_log(&format!(
            "oxide.renderer-metal: init begin simulator={} wants_hdr={} sample_count={} camera_mode={:?} camera_source={:?}",
            simulator,
            config.wants_hdr,
            config.sample_count,
            config.camera_render_mode,
            config.camera_texture_source
        ));
        ios_log("oxide.renderer-metal: init before device resolve");
        let device = if let Some(external) = take_external_mtl_device() {
            external
        } else {
            ios_log("oxide.renderer-metal: init before Device::system_default");
            let resolved = Device::system_default().ok_or(MetalInitError::NoDevice)?;
            ios_log("oxide.renderer-metal: init after Device::system_default");
            resolved
        };
        ios_log("oxide.renderer-metal: init after device resolve");
        let queue = device.new_command_queue();
        ios_log("oxide.renderer-metal: init after new_command_queue");
        let compile_opts = CompileOptions::new();
        // Target explicit Metal Shading Language version for cross-macOS consistency
        // Highest available in metal-rs 0.32.0 (MSL 3.2 not yet exposed)
        compile_opts.set_language_version(MTLLanguageVersion::V3_0);
        ios_log("oxide.renderer-metal: init before shader library compile");
        let library = device
            .new_library_with_source(SHADERS_SRC, &compile_opts)
            .map_err(|e| MetalInitError::Library(format!("{}", e)))?;
        ios_log("oxide.renderer-metal: init after shader library compile");
        let mut sample_count = apply_simulator_sample_count(simulator, config.sample_count);
        while sample_count > 1 && !device.supports_texture_sample_count(sample_count as u64) {
            sample_count = sample_count / 2;
        }
        if sample_count == 0 {
            sample_count = 1;
        }

        let mut hdr_enabled = apply_simulator_hdr(simulator, config.wants_hdr);
        let mut color_format =
            if hdr_enabled { MTLPixelFormat::BGRA10_XR } else { MTLPixelFormat::BGRA8Unorm_sRGB };

        let mut use_glyph_icb = apply_simulator_safety_bool(
            simulator,
            env_flag("OXIDE_GLYPH_USE_ICB").unwrap_or_else(glyph_icb_enabled_default),
        );

        let direct_preview_only = config.direct_preview_only;
        let build_pipelines = |fmt: MTLPixelFormat,
                               supports_glyph_icb: bool|
         -> Result<_, MetalInitError> {
            let pso_camera = build_init_stage("pso.camera_nv12", || {
                build_camera_pso(&device, &library, fmt, sample_count, "f_camera_nv12")
            })?;
            let pso_camera_legacy = build_init_stage("pso.camera_nv12_legacy", || {
                build_camera_pso(&device, &library, fmt, sample_count, "f_camera_nv12_legacy")
            })?;
            let pso_camera_preview_fast_full =
                build_init_stage("pso.camera_nv12_preview_fast_full", || {
                    build_camera_pso(
                        &device,
                        &library,
                        fmt,
                        sample_count,
                        "f_camera_nv12_preview_fast_full",
                    )
                })?;
            let pso_camera_preview_fast_video =
                build_init_stage("pso.camera_nv12_preview_fast_video", || {
                    build_camera_pso(
                        &device,
                        &library,
                        fmt,
                        sample_count,
                        "f_camera_nv12_preview_fast_video",
                    )
                })?;
            let pso_camera_bgra = build_init_stage("pso.camera_bgra_bench", || {
                build_camera_pso(&device, &library, fmt, sample_count, "f_camera_bgra_bench")
            })?;
            if direct_preview_only {
                return Ok((
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera.to_owned(),
                    pso_camera,
                    pso_camera_legacy,
                    pso_camera_preview_fast_full,
                    pso_camera_preview_fast_video,
                    pso_camera_bgra,
                ));
            }
            let pso_solid = build_init_stage("pso.solid", || {
                build_solid_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_image = build_init_stage("pso.image", || {
                build_image_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_image_single = build_init_stage("pso.image_single", || {
                build_image_single_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_blur = build_init_stage("pso.blur", || build_blur_pso(&device, &library, fmt))?;
            let pso_downsample = build_init_stage("pso.downsample", || {
                build_downsample_pso(&device, &library, fmt)
            })?;
            let pso_upsample =
                build_init_stage("pso.upsample", || build_upsample_pso(&device, &library, fmt))?;
            let pso_backdrop =
                build_init_stage("pso.backdrop", || build_backdrop_pso(&device, &library, fmt))?;
            let pso_rrect = build_init_stage("pso.rrect", || {
                build_rrect_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_nine = build_init_stage("pso.nine_slice", || {
                build_nine_slice_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_spin = build_init_stage("pso.spinner", || {
                build_spinner_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_text = build_init_stage("pso.text", || {
                build_text_pso(&device, &library, fmt, sample_count, supports_glyph_icb)
            })?;
            let pso_text_sdf = build_init_stage("pso.text_sdf", || {
                build_text_sdf_pso(&device, &library, fmt, sample_count, supports_glyph_icb)
            })?;
            Ok((
                pso_solid,
                pso_image,
                pso_image_single,
                pso_blur,
                pso_downsample,
                pso_upsample,
                pso_backdrop,
                pso_rrect,
                pso_nine,
                pso_spin,
                pso_text,
                pso_text_sdf,
                pso_camera,
                pso_camera_legacy,
                pso_camera_preview_fast_full,
                pso_camera_preview_fast_video,
                pso_camera_bgra,
            ))
        };

        let (
            pso_solid,
            pso_image,
            pso_image_single,
            pso_blur,
            pso_downsample,
            pso_upsample,
            pso_backdrop,
            pso_rrect,
            pso_nine,
            pso_spin,
            pso_text,
            pso_text_sdf,
            pso_camera,
            pso_camera_legacy,
            pso_camera_preview_fast_full,
            pso_camera_preview_fast_video,
            pso_camera_bgra,
        ) = loop {
            match build_pipelines(color_format, use_glyph_icb) {
                Ok(pipelines) => break pipelines,
                Err(err) => {
                    if hdr_enabled {
                        hdr_enabled = false;
                        color_format = MTLPixelFormat::BGRA8Unorm_sRGB;
                        continue;
                    }
                    if use_glyph_icb && pipeline_mentions_indirect_command_buffers(&err) {
                        eprintln!(
                            "[Oxide] renderer disabling glyph ICB after pipeline rejection: {err}"
                        );
                        use_glyph_icb = false;
                        continue;
                    }
                    return Err(err);
                }
            }
        };
        // Prepare argument encoder for image textures
        let (img_arg, img_arg_buf) = if direct_preview_only {
            (None, None)
        } else {
            let f_image_fn = pipeline_function(&library, "function.f_image", "f_image")?;
            let img_arg = Some(f_image_fn.new_argument_encoder(2));
            let img_ab_len = img_arg.as_ref().unwrap().encoded_length();
            let img_arg_buf =
                Some(device.new_buffer(img_ab_len, MTLResourceOptions::StorageModeShared));
            img_arg.as_ref().unwrap().set_argument_buffer(img_arg_buf.as_ref().unwrap(), 0);
            (img_arg, img_arg_buf)
        };
        let sampler = build_sampler(&device);
        let opts =
            MTLResourceOptions::CPUCacheModeWriteCombined | MTLResourceOptions::StorageModeShared;
        let direct_preview_ring_size = 4 * 1024;
        // Pre-size dynamic rings to reduce first-frame growth churn on Simulator.
        // This path previously hit MTLSim `newBuffer` failures during early growth.
        let vb = Ring::new(
            &device,
            if direct_preview_only { direct_preview_ring_size } else { 4 * 1024 * 1024 },
            opts,
        );
        let ib = Ring::new(
            &device,
            if direct_preview_only { direct_preview_ring_size } else { 2 * 1024 * 1024 },
            opts,
        );
        let ub = Ring::new(
            &device,
            if direct_preview_only { direct_preview_ring_size } else { 2 * 1024 * 1024 },
            opts,
        );
        let damage_enabled = !direct_preview_only
            && apply_simulator_safety_bool(
                simulator,
                env_flag("OXIDE_ENABLE_DAMAGE").unwrap_or(false),
            );
        let layer_cache_enabled = !direct_preview_only
            && apply_simulator_safety_bool(
                simulator,
                env_flag("OXIDE_ENABLE_LAYER_CACHE").unwrap_or_else(layer_cache_enabled_default),
            );
        let use_camera_textures = apply_simulator_safety_bool(
            simulator,
            env_flag("OXIDE_ENABLE_CAMERA_TEXTURES").unwrap_or(true),
        );
        let use_image_arg_buffer = !direct_preview_only
            && apply_simulator_safety_bool(
                simulator,
                env_flag("OXIDE_ENABLE_IMAGE_ARG_BUFFER").unwrap_or(true),
            );
        if !use_glyph_icb {
            ios_log("oxide.renderer-metal: glyph ICB path disabled");
        }
        if !layer_cache_enabled {
            ios_log("oxide.renderer-metal: layer cache path disabled");
        }
        if !use_camera_textures {
            ios_log("oxide.renderer-metal: camera texture path disabled");
        }
        if !use_image_arg_buffer {
            ios_log("oxide.renderer-metal: image argument-buffer path disabled");
        }
        let damage_use_thresh = std::env::var("OXIDE_DAMAGE_USE_THRESH")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.70);
        let damage_prefilter_thresh = std::env::var("OXIDE_DAMAGE_PREFILTER_THRESH")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.25);
        let applied_config = MetalRendererConfig {
            wants_hdr: hdr_enabled,
            sample_count,
            camera_render_mode: config.camera_render_mode,
            camera_texture_source: config.camera_texture_source,
            direct_preview_only,
        };
        let camera_preview_renderer = if direct_preview_only
            && experimental_tiny_camera_preview_renderer_enabled()
        {
            ios_log("oxide.renderer-metal: [EXPERIMENTAL] tiny camera preview renderer enabled");
            Some(CameraPreviewRenderer::new(
                queue.to_owned(),
                pso_camera.to_owned(),
                pso_camera_legacy.to_owned(),
                pso_camera_preview_fast_full.to_owned(),
                pso_camera_preview_fast_video.to_owned(),
            ))
        } else {
            None
        };

        Ok(Self {
            device,
            queue,
            pso_solid,
            pso_image,
            pso_image_single,
            pso_blur,
            pso_downsample,
            pso_upsample,
            pso_backdrop,
            pso_rrect,
            pso_nine_slice: pso_nine,
            pso_spinner: pso_spin,
            pso_text,
            pso_text_sdf,
            pso_camera,
            pso_camera_legacy,
            pso_camera_preview_fast_full,
            pso_camera_preview_fast_video,
            pso_camera_bgra,
            img_arg,
            img_arg_buf,
            sampler,
            color_format,
            config: applied_config,
            sample_count,
            hdr_enabled,
            frame_id: 0,
            frames: core::array::from_fn(|_| PerFrame::new()),
            vb,
            ib,
            ub,
            target_w: 0,
            target_h: 0,
            target_scale: 1.0,
            target_tex: None,
            target_msaa_tex: None,
            prepass_tex: None,
            blur_tmp_tex: None,
            half_tex: None,
            quarter_tex: None,
            quarter_tmp_tex: None,
            images: HashMap::new(),
            next_image_id: 1,
            layers: HashMap::new(),
            layer_cache_enabled,
            last_stats: PerfStats::default(),
            acc_draws: 0,
            acc_instanced: 0,
            acc_icb_cmds: 0,
            use_glyph_icb,
            damage_enabled,
            frame_scissor_dp: None,
            frame_damage_rects: 0,
            frame_damage_pct: 0.0,
            frame_damage_px: 0,
            acc_culled: 0,
            damage_use_thresh,
            damage_prefilter_thresh,
            main_shaded_px: 0,
            prepass_shaded_px: 0,
            scissor_changes: 0,
            cam_blur_tex: None,
            cam_last_update: None,
            cam_update_period: std::time::Duration::from_millis(83), // ~12 fps
            cam_paused: false,
            cam_pause_frames: 0,
            last_cam_w: 0,
            last_cam_h: 0,
            last_cam_bd: 8,
            last_cam_mx: 0,
            last_cam_vr: 0,
            last_cam_cs: 0,
            last_cam_fetch_ms: 0.0,
            cam_xfade_prev_tex: None,
            cam_xfade_t0: None,
            cam_xfade_ms: 120,
            cam_blur_fade_t0: None,
            camera_render_mode: config.camera_render_mode,
            camera_texture_source: config.camera_texture_source,
            current_live_camera_frame: None,
            camera_preview_renderer,
            bench_cam_y_tex: None,
            bench_cam_uv_tex: None,
            bench_cam_bgra_tex: None,
            use_camera_textures,
            use_image_arg_buffer,
            submit_error_flag: Arc::new(AtomicBool::new(false)),
            direct_preview_submitted: VecDeque::new(),
            direct_preview_last_submission_depth: 0,
            direct_preview_last_submission_skipped: 0,
            direct_preview_last_present_frame_age_ms: 0.0,
            direct_preview_last_completed_frame_id: 0,
            direct_preview_last_completed_gpu_ms: 0.0,
            pending_present_drawable: 0,
            pending_present_texture: 0,
        })
    }

    pub fn new_with_config(config: MetalRendererConfig) -> Result<Self, MetalInitError> {
        Self::new_with_config_impl(config)
    }

    pub fn new_default() -> Result<Self, MetalInitError> {
        Self::new_with_config(MetalRendererConfig::default())
    }

    pub fn set_camera_render_mode(&mut self, mode: CameraRenderMode) {
        self.camera_render_mode = mode;
        self.config.camera_render_mode = mode;
    }

    pub fn set_camera_texture_source(&mut self, source: CameraTextureSource) {
        if self.camera_texture_source != source {
            self.release_live_camera_frame();
        }
        self.camera_texture_source = source;
        self.config.camera_texture_source = source;
    }

    #[cfg(target_os = "ios")]
    fn release_live_camera_frame(&mut self) {
        extern "C" {
            fn oxide_cam_release_acquired(slot: u32, generation: u64);
        }
        if let Some(frame) = self.current_live_camera_frame.take() {
            unsafe {
                oxide_cam_release_acquired(frame.slot, frame.generation);
            }
        }
    }

    #[cfg(not(target_os = "ios"))]
    fn release_live_camera_frame(&mut self) {
        self.current_live_camera_frame = None;
    }

    fn poll_direct_preview_submissions(&mut self) {
        let log_enabled = ios_log_enabled();
        while let Some((frame_id, cmd)) = self.direct_preview_submitted.front() {
            let status = cmd.status();
            match status {
                MTLCommandBufferStatus::Completed => {
                    self.direct_preview_last_completed_frame_id = *frame_id;
                    self.direct_preview_last_completed_gpu_ms =
                        unsafe { command_buffer_gpu_duration_ms(cmd) };
                    self.direct_preview_submitted.pop_front();
                }
                MTLCommandBufferStatus::Error => {
                    if log_enabled {
                        unsafe {
                            let err: *mut Object = msg_send![cmd.as_ptr(), error];
                            if !err.is_null() {
                                let code: i64 = msg_send![err, code];
                                let domain_obj: *mut Object = msg_send![err, domain];
                                let desc_obj: *mut Object = msg_send![err, localizedDescription];
                                let domain = nsstring_to_string(domain_obj)
                                    .unwrap_or_else(|| "<null-domain>".to_string());
                                let desc = nsstring_to_string(desc_obj)
                                    .unwrap_or_else(|| "<null-description>".to_string());
                                ios_log(&format!(
                                    "oxide.renderer-metal: direct preview submit error frame={} domain={} code={} desc={}",
                                    frame_id, domain, code, desc
                                ));
                            } else {
                                ios_log(&format!(
                                    "oxide.renderer-metal: direct preview submit error frame={} error=nil",
                                    frame_id
                                ));
                            }
                        }
                    }
                    self.submit_error_flag.store(true, Ordering::Release);
                    self.direct_preview_submitted.pop_front();
                }
                MTLCommandBufferStatus::Committed
                | MTLCommandBufferStatus::Scheduled
                | MTLCommandBufferStatus::Enqueued
                | MTLCommandBufferStatus::NotEnqueued => break,
            }
        }
    }

    fn track_direct_preview_submission(&mut self, frame_id: u64, cmd: &CommandBuffer) {
        self.direct_preview_submitted.push_back((frame_id, cmd.to_owned()));
    }

    #[inline]
    fn note_direct_preview_submission_depth(&mut self) -> u32 {
        let depth = self.direct_preview_submitted.len() as u32;
        self.direct_preview_last_submission_depth = depth;
        depth
    }

    #[inline]
    fn direct_preview_backpressure_blocks_present(&mut self) -> bool {
        let depth = self.note_direct_preview_submission_depth() as usize;
        let blocked = self.camera_preview_renderer.is_none()
            && direct_preview_submission_backpressure_applies(
                experimental_preview_submission_backpressure_enabled(),
                depth,
            );
        self.direct_preview_last_submission_skipped = if blocked { 1 } else { 0 };
        blocked
    }

    #[cfg(target_os = "ios")]
    fn fetch_live_camera_nv12_if_new(
        &self,
        min_generation_exclusive: u64,
    ) -> Option<LiveCameraNv12Frame> {
        #[repr(C)]
        #[derive(Clone, Copy, Debug)]
        struct OxideCamAcquiredFrame {
            y_tex: *mut core::ffi::c_void,
            uv_tex: *mut core::ffi::c_void,
            width: i32,
            height: i32,
            bit_depth: i32,
            matrix: i32,
            video_range: i32,
            color_space: i32,
            slot: u32,
            generation: u64,
            timestamp_ns: u64,
        }
        impl Default for OxideCamAcquiredFrame {
            fn default() -> Self {
                Self {
                    y_tex: core::ptr::null_mut(),
                    uv_tex: core::ptr::null_mut(),
                    width: 0,
                    height: 0,
                    bit_depth: 0,
                    matrix: 0,
                    video_range: 0,
                    color_space: 0,
                    slot: 0,
                    generation: 0,
                    timestamp_ns: 0,
                }
            }
        }

        extern "C" {
            fn oxide_cam_acquire_latest_frame_ex(
                min_generation_exclusive: u64,
                out_frame: *mut OxideCamAcquiredFrame,
            ) -> ::libc::c_int;
        }

        let mut acquired = OxideCamAcquiredFrame::default();
        let ok =
            unsafe { oxide_cam_acquire_latest_frame_ex(min_generation_exclusive, &mut acquired) };
        if ok == 0
            || acquired.y_tex.is_null()
            || acquired.uv_tex.is_null()
            || acquired.width <= 0
            || acquired.height <= 0
        {
            return None;
        }
        Some(LiveCameraNv12Frame {
            y_tex: acquired.y_tex as usize,
            uv_tex: acquired.uv_tex as usize,
            width: acquired.width,
            height: acquired.height,
            bit_depth: acquired.bit_depth,
            matrix: acquired.matrix,
            video_range: acquired.video_range,
            color_space: acquired.color_space,
            slot: acquired.slot,
            generation: acquired.generation,
            timestamp_ns: acquired.timestamp_ns,
        })
    }

    #[cfg(target_os = "ios")]
    fn peek_live_camera_frame_identity(&self) -> (u64, u64) {
        extern "C" {
            fn oxide_cam_peek_latest_generation() -> u64;
            fn oxide_cam_peek_latest_timestamp_ns() -> u64;
        }
        unsafe { (oxide_cam_peek_latest_generation(), oxide_cam_peek_latest_timestamp_ns()) }
    }

    #[cfg(not(target_os = "ios"))]
    fn peek_live_camera_frame_identity(&self) -> (u64, u64) {
        (0, 0)
    }

    #[cfg(not(target_os = "ios"))]
    fn fetch_live_camera_nv12_if_new(
        &self,
        _min_generation_exclusive: u64,
    ) -> Option<LiveCameraNv12Frame> {
        None
    }

    fn fetch_live_camera_nv12(&self) -> Option<CameraNv12Source> {
        #[cfg(target_os = "ios")]
        {
            extern "C" {
                fn oxide_cam_get_latest_ex(
                    y_tex: *mut *mut core::ffi::c_void,
                    uv_tex: *mut *mut core::ffi::c_void,
                    w: *mut i32,
                    h: *mut i32,
                    bitdepth: *mut i32,
                    matrix: *mut i32,
                    video_range: *mut i32,
                    colorspace: *mut i32,
                ) -> ::libc::c_int;
            }
            let (
                mut y_tex,
                mut uv_tex,
                mut width,
                mut height,
                mut bit_depth,
                mut matrix,
                mut video_range,
                mut color_space,
            ) = (core::ptr::null_mut(), core::ptr::null_mut(), 0i32, 0i32, 0i32, 0i32, 0i32, 0i32);
            let ok = unsafe {
                oxide_cam_get_latest_ex(
                    &mut y_tex,
                    &mut uv_tex,
                    &mut width,
                    &mut height,
                    &mut bit_depth,
                    &mut matrix,
                    &mut video_range,
                    &mut color_space,
                )
            };
            if ok == 0 || y_tex.is_null() || uv_tex.is_null() || width <= 0 || height <= 0 {
                return None;
            }
            Some(CameraNv12Source {
                y_tex: unsafe { Texture::from_ptr(y_tex as *mut MTLTexture) },
                uv_tex: unsafe { Texture::from_ptr(uv_tex as *mut MTLTexture) },
                width,
                height,
                bit_depth,
                matrix,
                video_range,
                color_space,
            })
        }
        #[cfg(not(target_os = "ios"))]
        {
            None
        }
    }

    #[cfg(target_os = "ios")]
    fn fetch_live_camera_bgra(&self) -> Option<CameraBgraSource> {
        extern "C" {
            fn oxide_cam_get_latest_bgra(
                bgra_tex: *mut *mut core::ffi::c_void,
                w: *mut i32,
                h: *mut i32,
            ) -> ::libc::c_int;
        }

        let mut bgra_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        let ok = unsafe { oxide_cam_get_latest_bgra(&mut bgra_ptr, &mut width, &mut height) };
        if ok == 0 || bgra_ptr.is_null() || width <= 0 || height <= 0 {
            return None;
        }
        Some(CameraBgraSource {
            tex: unsafe { Texture::from_ptr(bgra_ptr as *mut MTLTexture) },
            width,
            height,
        })
    }

    #[cfg(not(target_os = "ios"))]
    fn fetch_live_camera_bgra(&self) -> Option<CameraBgraSource> {
        None
    }

    fn fetch_camera_nv12(&mut self) -> Option<CameraNv12Source> {
        if self.camera_texture_source == CameraTextureSource::SyntheticBenchmark {
            self.ensure_benchmark_camera_textures();
            return Some(CameraNv12Source {
                y_tex: self.bench_cam_y_tex.as_ref()?.to_owned(),
                uv_tex: self.bench_cam_uv_tex.as_ref()?.to_owned(),
                width: 1920,
                height: 1080,
                bit_depth: 8,
                matrix: 0,
                video_range: 0,
                color_space: 0,
            });
        }
        self.fetch_live_camera_nv12()
    }

    fn fetch_camera_bgra(&mut self) -> Option<CameraBgraSource> {
        if self.camera_texture_source == CameraTextureSource::Live {
            return self.fetch_live_camera_bgra();
        }
        self.ensure_benchmark_camera_textures();
        Some(CameraBgraSource {
            tex: self.bench_cam_bgra_tex.as_ref()?.to_owned(),
            width: 1920,
            height: 1080,
        })
    }

    fn encode_camera_quad(
        &mut self,
        enc: &RenderCommandEncoderRef,
        vp_dp: [f32; 2],
        rect_dp: [f32; 4],
        tint: api::Color,
        alpha: f32,
        grayscale: bool,
    ) -> Option<(i32, i32, i32, i32, i32, i32)> {
        let collect_stage_stats = camera_perf_stage_stats_enabled();
        enc.set_vertex_bytes(1, core::mem::size_of_val(&vp_dp) as u64, vp_dp.as_ptr() as *const _);
        enc.set_vertex_bytes(
            0,
            core::mem::size_of_val(&rect_dp) as u64,
            rect_dp.as_ptr() as *const _,
        );
        if let Some(sam) = &self.sampler {
            enc.set_fragment_sampler_state(0, Some(sam));
        }

        match self.camera_render_mode {
            CameraRenderMode::BgraBenchmark => {
                let fetch_t0 = collect_stage_stats.then(Instant::now);
                let src = self.fetch_camera_bgra()?;
                self.last_cam_fetch_ms = elapsed_ms(fetch_t0);
                let (uv_scale, uv_bias) =
                    camera_aspect_fill_params(rect_dp[2], rect_dp[3], src.width, src.height);
                let params =
                    pack_camera_params(rect_dp, tint, alpha, uv_scale, uv_bias, grayscale, 0, 0, 8);
                enc.set_render_pipeline_state(&self.pso_camera_bgra);
                enc.set_fragment_texture(0, Some(&src.tex));
                enc.set_fragment_texture(1, None);
                enc.set_fragment_bytes(
                    1,
                    core::mem::size_of_val(&params) as u64,
                    (&params as *const CameraGpuParams).cast(),
                );
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                Some((src.width, src.height, 8, 0, 0, 0))
            }
            CameraRenderMode::Nv12Optimized | CameraRenderMode::Nv12Legacy => {
                let fetch_t0 = collect_stage_stats.then(Instant::now);
                let src = self.fetch_camera_nv12()?;
                self.last_cam_fetch_ms = elapsed_ms(fetch_t0);
                let (uv_scale, uv_bias) =
                    camera_aspect_fill_params(rect_dp[2], rect_dp[3], src.width, src.height);
                match self.camera_render_mode {
                    CameraRenderMode::Nv12Optimized => {
                        let params = pack_camera_params(
                            rect_dp,
                            tint,
                            alpha,
                            uv_scale,
                            uv_bias,
                            grayscale,
                            src.matrix,
                            src.video_range,
                            src.bit_depth,
                        );
                        enc.set_render_pipeline_state(&self.pso_camera);
                        enc.set_fragment_bytes(
                            1,
                            core::mem::size_of_val(&params) as u64,
                            (&params as *const CameraGpuParams).cast(),
                        );
                    }
                    CameraRenderMode::Nv12Legacy => {
                        let params = pack_camera_params(
                            rect_dp,
                            tint,
                            alpha,
                            uv_scale,
                            uv_bias,
                            grayscale,
                            src.matrix,
                            src.video_range,
                            src.bit_depth,
                        );
                        enc.set_render_pipeline_state(&self.pso_camera_legacy);
                        enc.set_fragment_bytes(
                            1,
                            core::mem::size_of_val(&params) as u64,
                            (&params as *const CameraGpuParams).cast(),
                        );
                    }
                    CameraRenderMode::BgraBenchmark => unreachable!(),
                }
                enc.set_fragment_texture(0, Some(&src.y_tex));
                enc.set_fragment_texture(1, Some(&src.uv_tex));
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                Some((
                    src.width,
                    src.height,
                    src.bit_depth,
                    src.matrix,
                    src.video_range,
                    src.color_space,
                ))
            }
        }
    }

    fn ensure_target(&mut self) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        let need_new = match &self.target_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        if need_new {
            let desc = TextureDescriptor::new();
            desc.set_pixel_format(self.color_format);
            desc.set_texture_type(MTLTextureType::D2);
            desc.set_width(self.target_w as u64);
            desc.set_height(self.target_h as u64);
            desc.set_storage_mode(MTLStorageMode::Private);
            desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.target_tex = Some(self.device.new_texture(&desc));
        }

        if self.sample_count > 1 {
            let need_msaa = match &self.target_msaa_tex {
                Some(tex) => {
                    tex.width() as u32 != self.target_w
                        || tex.height() as u32 != self.target_h
                        || tex.sample_count() != self.sample_count as u64
                }
                None => true,
            };
            if need_msaa {
                let desc = TextureDescriptor::new();
                desc.set_pixel_format(self.color_format);
                desc.set_texture_type(MTLTextureType::D2Multisample);
                desc.set_width(self.target_w as u64);
                desc.set_height(self.target_h as u64);
                desc.set_storage_mode(MTLStorageMode::Private);
                desc.set_usage(MTLTextureUsage::RenderTarget);
                desc.set_sample_count(self.sample_count as u64);
                self.target_msaa_tex = Some(self.device.new_texture(&desc));
            }
        } else {
            self.target_msaa_tex = None;
        }
    }

    fn drop_direct_preview_offscreen_targets(&mut self) {
        self.target_tex = None;
        self.target_msaa_tex = None;
        self.prepass_tex = None;
        self.blur_tmp_tex = None;
        self.half_tex = None;
        self.quarter_tex = None;
        self.quarter_tmp_tex = None;
        self.cam_blur_tex = None;
        self.cam_xfade_prev_tex = None;
    }

    pub fn resize_for_direct_preview(&mut self, w: u32, h: u32, scale: f32) {
        let target_w = w.max(1);
        let target_h = h.max(1);
        let target_scale = if scale > 0.0 { scale } else { 1.0 };
        if direct_preview_can_reuse_resize_targets(
            self.target_w,
            self.target_h,
            self.target_scale,
            target_w,
            target_h,
            target_scale,
            self.sample_count,
        ) {
            return;
        }
        self.target_w = target_w;
        self.target_h = target_h;
        self.target_scale = target_scale;
        if self.sample_count == 1 {
            self.drop_direct_preview_offscreen_targets();
        }
    }

    fn ensure_effect_targets(&mut self) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        let need_src = match &self.prepass_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        let need_tmp = match &self.blur_tmp_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        if need_src {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(self.target_w as u64);
            d.set_height(self.target_h as u64);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.prepass_tex = Some(self.device.new_texture(&d));
        }
        if need_tmp {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(self.target_w as u64);
            d.set_height(self.target_h as u64);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.blur_tmp_tex = Some(self.device.new_texture(&d));
        }

        // Downsample chain targets (half, quarter) + quarter ping-pong
        let (hw, hh) = (((self.target_w / 2).max(1)) as u64, ((self.target_h / 2).max(1)) as u64);
        let (qw, qh) = (((self.target_w / 4).max(1)) as u64, ((self.target_h / 4).max(1)) as u64);
        let need_half = match &self.half_tex {
            Some(tex) => tex.width() != hw || tex.height() != hh,
            None => true,
        };
        let need_quarter = match &self.quarter_tex {
            Some(tex) => tex.width() != qw || tex.height() != qh,
            None => true,
        };
        let need_quarter_tmp = match &self.quarter_tmp_tex {
            Some(tex) => tex.width() != qw || tex.height() != qh,
            None => true,
        };
        if need_half {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(hw);
            d.set_height(hh);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.half_tex = Some(self.device.new_texture(&d));
        }
        if need_quarter {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(qw);
            d.set_height(qh);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.quarter_tex = Some(self.device.new_texture(&d));
        }
        if need_quarter_tmp {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(qw);
            d.set_height(qh);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.quarter_tmp_tex = Some(self.device.new_texture(&d));
        }
    }

    fn get_image_tex(&self, h: api::ImageHandle) -> Option<&Texture> {
        self.images.get(&h.0)
    }

    pub fn image_create_a8(
        &mut self,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::R8Unorm);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_storage_mode(MTLStorageMode::Shared);
        desc.set_usage(MTLTextureUsage::ShaderRead);
        let tex = self.device.new_texture(&desc);
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
        };
        let bpr = if row_bytes == 0 { w as usize } else { row_bytes } as u64;
        tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.images.insert(id, tex);
        api::ImageHandle(id)
    }

    pub fn image_update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        if let Some(tex) = self.images.get(&handle.0) {
            let region = MTLRegion {
                origin: MTLOrigin { x: x as u64, y: y as u64, z: 0 },
                size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
            };
            let bpr = if row_bytes == 0 { w as usize } else { row_bytes } as u64;
            tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        }
    }

    pub fn image_create_rgba8(
        &mut self,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_storage_mode(MTLStorageMode::Shared);
        desc.set_usage(MTLTextureUsage::ShaderRead);
        let tex = self.device.new_texture(&desc);
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
        };
        let bpr = if row_bytes == 0 { (w as usize) * 4 } else { row_bytes } as u64;
        tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.images.insert(id, tex);
        api::ImageHandle(id)
    }

    pub fn image_update_rgba8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        if let Some(tex) = self.images.get(&handle.0) {
            let region = MTLRegion {
                origin: MTLOrigin { x: x as u64, y: y as u64, z: 0 },
                size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
            };
            let bpr = if row_bytes == 0 { (w as usize) * 4 } else { row_bytes } as u64;
            tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        }
    }

    pub fn image_release(&mut self, handle: api::ImageHandle) {
        let _ = self.images.remove(&handle.0);
    }

    fn ensure_benchmark_camera_textures(&mut self) {
        if self.bench_cam_y_tex.is_some()
            && self.bench_cam_uv_tex.is_some()
            && self.bench_cam_bgra_tex.is_some()
        {
            return;
        }

        let width = 1920u32;
        let height = 1080u32;
        let chroma_width = width / 2;
        let chroma_height = height / 2;
        let mut y_plane = vec![0u8; (width * height) as usize];
        let mut uv_plane = vec![0u8; (chroma_width * chroma_height * 2) as usize];
        let mut bgra = vec![0u8; (width * height * 4) as usize];

        for y in 0..height {
            for x in 0..width {
                let fx = x as f32 / (width.saturating_sub(1) as f32).max(1.0);
                let fy = y as f32 / (height.saturating_sub(1) as f32).max(1.0);
                let stripe = (((x / 32) ^ (y / 24)) & 1) as f32;
                let wave = (((fx * core::f32::consts::TAU * 3.0).sin() * 0.5 + 0.5)
                    + ((fy * core::f32::consts::TAU * 2.0).cos() * 0.5 + 0.5))
                    * 0.5;
                let luma = (0.18 + wave * 0.62 + stripe * 0.08).clamp(0.0, 1.0);
                y_plane[(y * width + x) as usize] = (luma * 255.0).round() as u8;
            }
        }

        for y in 0..chroma_height {
            for x in 0..chroma_width {
                let fx = x as f32 / (chroma_width.saturating_sub(1) as f32).max(1.0);
                let fy = y as f32 / (chroma_height.saturating_sub(1) as f32).max(1.0);
                let cb = (128.0
                    + (fx * core::f32::consts::TAU * 1.5).sin() * 42.0
                    + (fy * core::f32::consts::TAU).cos() * 18.0)
                    .clamp(16.0, 240.0);
                let cr = (128.0 + (fy * core::f32::consts::TAU * 1.25).sin() * 38.0
                    - (fx * core::f32::consts::TAU * 0.75).cos() * 22.0)
                    .clamp(16.0, 240.0);
                let offset = ((y * chroma_width + x) * 2) as usize;
                uv_plane[offset] = cb.round() as u8;
                uv_plane[offset + 1] = cr.round() as u8;
            }
        }

        for y in 0..height {
            for x in 0..width {
                let y_code = y_plane[(y * width + x) as usize] as f32 / 255.0;
                let uv_index = (((y / 2) * chroma_width + (x / 2)) * 2) as usize;
                let cb_code = uv_plane[uv_index] as f32 / 255.0;
                let cr_code = uv_plane[uv_index + 1] as f32 / 255.0;
                let u = cb_code - (128.0 / 255.0);
                let v = cr_code - (128.0 / 255.0);
                let rgb = yuv_to_rgb_bt709_full_range(y_code, u, v);
                let offset = ((y * width + x) * 4) as usize;
                bgra[offset] = linear_to_srgb_u8(rgb[2]);
                bgra[offset + 1] = linear_to_srgb_u8(rgb[1]);
                bgra[offset + 2] = linear_to_srgb_u8(rgb[0]);
                bgra[offset + 3] = 255;
            }
        }

        let y_desc = TextureDescriptor::new();
        y_desc.set_pixel_format(MTLPixelFormat::R8Unorm);
        y_desc.set_texture_type(MTLTextureType::D2);
        y_desc.set_width(width as u64);
        y_desc.set_height(height as u64);
        y_desc.set_storage_mode(MTLStorageMode::Shared);
        y_desc.set_usage(MTLTextureUsage::ShaderRead);
        let y_tex = self.device.new_texture(&y_desc);
        let y_region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: width as u64, height: height as u64, depth: 1 },
        };
        y_tex.replace_region(y_region, 0, y_plane.as_ptr() as *const _, width as u64);

        let uv_desc = TextureDescriptor::new();
        uv_desc.set_pixel_format(MTLPixelFormat::RG8Unorm);
        uv_desc.set_texture_type(MTLTextureType::D2);
        uv_desc.set_width(chroma_width as u64);
        uv_desc.set_height(chroma_height as u64);
        uv_desc.set_storage_mode(MTLStorageMode::Shared);
        uv_desc.set_usage(MTLTextureUsage::ShaderRead);
        let uv_tex = self.device.new_texture(&uv_desc);
        let uv_region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: chroma_width as u64, height: chroma_height as u64, depth: 1 },
        };
        uv_tex.replace_region(
            uv_region,
            0,
            uv_plane.as_ptr() as *const _,
            (chroma_width * 2) as u64,
        );

        let bgra_desc = TextureDescriptor::new();
        bgra_desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
        bgra_desc.set_texture_type(MTLTextureType::D2);
        bgra_desc.set_width(width as u64);
        bgra_desc.set_height(height as u64);
        bgra_desc.set_storage_mode(MTLStorageMode::Shared);
        bgra_desc.set_usage(MTLTextureUsage::ShaderRead);
        let bgra_tex = self.device.new_texture(&bgra_desc);
        let bgra_region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: width as u64, height: height as u64, depth: 1 },
        };
        bgra_tex.replace_region(bgra_region, 0, bgra.as_ptr() as *const _, (width * 4) as u64);

        self.bench_cam_y_tex = Some(y_tex);
        self.bench_cam_uv_tex = Some(uv_tex);
        self.bench_cam_bgra_tex = Some(bgra_tex);
    }

    fn refresh_live_camera_preview_frame(&mut self) {
        if self.camera_texture_source != CameraTextureSource::Live {
            self.release_live_camera_frame();
            return;
        }
        let min_generation =
            self.current_live_camera_frame.as_ref().map(|frame| frame.generation).unwrap_or(0);
        if let Some(frame) = self.fetch_live_camera_nv12_if_new(min_generation) {
            self.release_live_camera_frame();
            self.current_live_camera_frame = Some(frame);
        }
    }

    fn direct_preview_camera_pipeline_for_frame(
        &self,
        frame: &LiveCameraNv12Frame,
    ) -> &RenderPipelineState {
        if direct_preview_uses_fast_yuv_pipeline(frame.bit_depth, frame.matrix, frame.video_range) {
            if frame.video_range == 0 {
                return &self.pso_camera_preview_fast_full;
            }
            if frame.video_range == 1 {
                return &self.pso_camera_preview_fast_video;
            }
        }
        match self.camera_render_mode {
            CameraRenderMode::Nv12Legacy => &self.pso_camera_legacy,
            CameraRenderMode::Nv12Optimized | CameraRenderMode::BgraBenchmark => &self.pso_camera,
        }
    }

    pub fn camera_preview_needs_drawable(
        &self,
        w: u32,
        h: u32,
        scale: f32,
        camera_running: bool,
    ) -> bool {
        direct_preview_reason_requires_drawable(self.camera_preview_draw_reason(
            w,
            h,
            scale,
            camera_running,
        ))
    }

    pub fn camera_preview_draw_reason(
        &self,
        w: u32,
        h: u32,
        scale: f32,
        camera_running: bool,
    ) -> u32 {
        if self
            .camera_preview_renderer
            .as_ref()
            .is_some_and(CameraPreviewRenderer::submit_error_pending)
        {
            return CAMERA_PREVIEW_REASON_SUBMIT_ERROR;
        }
        if self.submit_error_flag.load(Ordering::Acquire) {
            return CAMERA_PREVIEW_REASON_SUBMIT_ERROR;
        }
        if !self.config.direct_preview_only || self.sample_count > 1 {
            return CAMERA_PREVIEW_REASON_NON_DIRECT_PREVIEW;
        }
        let next_w = w.max(1);
        let next_h = h.max(1);
        let next_scale = if scale > 0.0 { scale } else { 1.0 };
        let resize_reused = direct_preview_can_reuse_resize_targets(
            self.target_w,
            self.target_h,
            self.target_scale,
            next_w,
            next_h,
            next_scale,
            self.sample_count,
        );
        if !resize_reused {
            return CAMERA_PREVIEW_REASON_RESIZE;
        }
        if !camera_running {
            return CAMERA_PREVIEW_REASON_CAMERA_STOPPED;
        }
        if self.camera_texture_source != CameraTextureSource::Live {
            return CAMERA_PREVIEW_REASON_NON_LIVE_SOURCE;
        }
        if !matches!(
            self.camera_render_mode,
            CameraRenderMode::Nv12Optimized | CameraRenderMode::Nv12Legacy
        ) {
            return CAMERA_PREVIEW_REASON_NON_NV12_MODE;
        }
        let current_generation =
            self.current_live_camera_frame.as_ref().map(|frame| frame.generation).unwrap_or(0);
        let current_timestamp_ns =
            self.current_live_camera_frame.as_ref().map(|frame| frame.timestamp_ns).unwrap_or(0);
        let (latest_generation, latest_timestamp_ns) = self.peek_live_camera_frame_identity();
        let reason = direct_live_preview_needs_render(
            resize_reused,
            self.current_live_camera_frame.is_some(),
            current_generation,
            current_timestamp_ns,
            latest_generation,
            latest_timestamp_ns,
        );
        if reason != 0
            && self.camera_preview_renderer.is_none()
            && direct_preview_submission_backpressure_applies(
                experimental_preview_submission_backpressure_enabled(),
                self.direct_preview_submitted.len(),
            )
        {
            return CAMERA_PREVIEW_REASON_BACKPRESSURE;
        }
        reason
    }

    fn encode_camera_quad_from_live_frame(
        &self,
        enc: &RenderCommandEncoderRef,
        frame: &LiveCameraNv12Frame,
        vp_dp: [f32; 2],
        rect_dp: [f32; 4],
        tint: api::Color,
        alpha: f32,
        grayscale: bool,
        collect_stage_stats: bool,
    ) -> DirectCameraEncodeStats {
        let bind_t0 = collect_stage_stats.then(Instant::now);
        let (uv_scale, uv_bias) =
            camera_aspect_fill_params(rect_dp[2], rect_dp[3], frame.width, frame.height);
        let params = pack_camera_params(
            rect_dp,
            tint,
            alpha,
            uv_scale,
            uv_bias,
            grayscale,
            frame.matrix,
            frame.video_range,
            frame.bit_depth,
        );
        let y_tex = unsafe { TextureRef::from_ptr(frame.y_tex as *mut MTLTexture) };
        let uv_tex = unsafe { TextureRef::from_ptr(frame.uv_tex as *mut MTLTexture) };
        with_perf_signpost("camera.renderer.direct.encode.bind", || {
            enc.set_render_pipeline_state(self.direct_preview_camera_pipeline_for_frame(frame));
            enc.set_fragment_texture(0, Some(y_tex));
            enc.set_fragment_texture(1, Some(uv_tex));
            enc.set_fragment_bytes(
                1,
                core::mem::size_of_val(&params) as u64,
                (&params as *const CameraGpuParams).cast(),
            );
        });
        let bind_ms = elapsed_ms(bind_t0);
        let draw_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost("camera.renderer.direct.encode.draw", || {
            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
        });
        DirectCameraEncodeStats {
            camera_width: frame.width,
            camera_height: frame.height,
            camera_bit_depth: frame.bit_depth,
            camera_matrix: frame.matrix,
            camera_video_range: frame.video_range,
            camera_color_space: frame.color_space,
            bind_ms,
            draw_ms: elapsed_ms(draw_t0),
        }
    }

    pub unsafe fn blit_to_texture_and_present_drawable(
        &mut self,
        dst_tex_ptr: *mut core::ffi::c_void,
        drawable_ptr: *mut core::ffi::c_void,
    ) -> Result<(), api::RenderError> {
        ios_log(&format!(
            "metal: blit+present begin dst={:p} drawable={:p}",
            dst_tex_ptr, drawable_ptr
        ));
        let src = match &self.target_tex {
            Some(t) => t,
            None => return Err(api::RenderError::InvalidOperation("no target texture")),
        };
        let raw_drawable_obj = drawable_ptr as *mut Object;
        let dst = if dst_tex_ptr.is_null() {
            let raw_dst_tex: *mut MTLTexture = unsafe { msg_send![raw_drawable_obj, texture] };
            if raw_dst_tex.is_null() {
                return Err(api::RenderError::InvalidOperation(
                    "drawable did not provide a destination texture",
                ));
            }
            unsafe { TextureRef::from_ptr(raw_dst_tex) }
        } else {
            unsafe { TextureRef::from_ptr(dst_tex_ptr as *mut MTLTexture) }
        };
        let raw_drawable = drawable_ptr as *mut MTLDrawable;
        let drawable = unsafe { DrawableRef::from_ptr(raw_drawable) };
        let cmd = self.queue.new_command_buffer();
        let blit = cmd.new_blit_command_encoder();
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let src_w = src.width();
        let src_h = src.height();
        let dst_w = dst.width();
        let dst_h = dst.height();
        let copy_w = src_w.min(dst_w);
        let copy_h = src_h.min(dst_h);
        if copy_w == 0 || copy_h == 0 {
            return Err(api::RenderError::InvalidOperation("zero-sized blit copy extent"));
        }
        if copy_w != src_w || copy_h != src_h {
            ios_log(&format!(
                "metal: clamped blit extent src={}x{} dst={}x{} copy={}x{}",
                src_w, src_h, dst_w, dst_h, copy_w, copy_h
            ));
        }
        let size = MTLSize { width: copy_w, height: copy_h, depth: 1 };
        blit.copy_from_texture(src, 0, 0, origin, size, dst, 0, 0, origin);
        blit.end_encoding();
        ios_log("metal: calling present_drawable");
        cmd.present_drawable(drawable);
        if ios_log_enabled() {
            let completion = ConcreteBlock::new(move |buffer: &CommandBufferRef| {
                ios_log(&format!("metal: present completion status={:?}", buffer.status()));
            })
            .copy();
            cmd.add_completed_handler(&completion);
        }
        ios_log("metal: committing command buffer");
        cmd.commit();
        ios_log("metal: blit+present end");
        Ok(())
    }

    pub unsafe fn prepare_present_drawable(
        &mut self,
        drawable_ptr: *mut core::ffi::c_void,
    ) -> Result<(), api::RenderError> {
        if drawable_ptr.is_null() {
            self.pending_present_drawable = 0;
            self.pending_present_texture = 0;
            return Ok(());
        }
        let raw_drawable_obj = drawable_ptr as *mut Object;
        let raw_dst_tex: *mut MTLTexture = msg_send![raw_drawable_obj, texture];
        if raw_dst_tex.is_null() {
            return Err(api::RenderError::InvalidOperation(
                "drawable did not provide a destination texture",
            ));
        }
        self.pending_present_drawable = drawable_ptr as usize;
        self.pending_present_texture = raw_dst_tex as usize;
        Ok(())
    }

    pub fn cancel_present_drawable(&mut self) -> *mut core::ffi::c_void {
        let drawable = self.pending_present_drawable as *mut core::ffi::c_void;
        self.pending_present_drawable = 0;
        self.pending_present_texture = 0;
        drawable
    }

    pub unsafe fn render_camera_preview_direct(
        &mut self,
        drawable_ptr: *mut core::ffi::c_void,
        w: u32,
        h: u32,
        scale: f32,
    ) -> Result<PerfStats, api::RenderError> {
        let collect_stage_stats = camera_perf_stage_stats_enabled();
        self.pending_present_drawable = 0;
        self.pending_present_texture = 0;
        if drawable_ptr.is_null() || self.sample_count > 1 {
            with_perf_signpost("camera.renderer.resize", || {
                <Self as api::Renderer>::resize(self, w, h, scale)
            })?;
        } else {
            with_perf_signpost("camera.renderer.resize", || {
                self.resize_for_direct_preview(w, h, scale)
            });
        }

        let cpu_t0 = collect_stage_stats.then(Instant::now);
        self.frame_id = self.frame_id.wrapping_add(1);
        self.acc_draws = 0;
        self.acc_instanced = 0;
        self.acc_icb_cmds = 0;
        self.acc_culled = 0;
        self.last_cam_fetch_ms = 0.0;
        let mut poll_submissions_ms = 0.0;
        self.direct_preview_last_submission_depth = 0;
        self.direct_preview_last_submission_skipped = 0;
        self.direct_preview_last_present_frame_age_ms = 0.0;

        if let Some(renderer) = self.camera_preview_renderer.as_ref() {
            if renderer.take_submit_error() {
                return Err(api::RenderError::DeviceLost);
            }
        } else {
            let poll_t0 = collect_stage_stats.then(Instant::now);
            with_perf_signpost("camera.renderer.direct.poll_submissions", || {
                self.poll_direct_preview_submissions();
            });
            poll_submissions_ms = elapsed_ms(poll_t0);
            if self.submit_error_flag.swap(false, Ordering::AcqRel) {
                return Err(api::RenderError::DeviceLost);
            }
        }

        let use_tiny_live_preview = direct_preview_tiny_renderer_active(
            self.camera_preview_renderer.is_some(),
            self.sample_count,
            self.camera_texture_source,
            self.camera_render_mode,
        );
        if use_tiny_live_preview {
            let fetch_t0 = collect_stage_stats.then(Instant::now);
            self.refresh_live_camera_preview_frame();
            self.last_cam_fetch_ms = elapsed_ms(fetch_t0);
            let current_frame = self.current_live_camera_frame.clone();
            let preview = self
                .camera_preview_renderer
                .as_mut()
                .expect("tiny preview renderer available for active tiny preview path")
                .render_live_frame(
                    drawable_ptr,
                    current_frame.as_ref(),
                    w,
                    h,
                    scale,
                    self.camera_render_mode,
                    collect_stage_stats,
                )?;
            self.last_cam_w = preview.camera_width.max(0);
            self.last_cam_h = preview.camera_height.max(0);
            self.last_cam_bd = preview.camera_bit_depth.max(0);
            self.last_cam_mx = preview.camera_matrix.max(0);
            self.last_cam_vr = preview.camera_video_range.max(0);
            self.last_cam_cs = preview.camera_color_space.max(0);
            self.acc_draws = if preview.drew_live_frame { 1 } else { 0 };
            self.last_stats = PerfStats {
                memory: self.memory_stats(),
                draws: self.acc_draws,
                instanced: self.acc_instanced,
                icb_cmds: self.acc_icb_cmds,
                encode_ms: elapsed_ms(cpu_t0),
                damage_px: 0,
                damage_pct: 0.0,
                damage_rects: 0,
                culled: self.acc_culled,
                blur_ms: 0.0,
                blur_updates: 0,
                blur_period_ms: 0,
                cam_coverage_pct: if preview.drew_live_frame { 1.0 } else { 0.0 },
                cam_paused: 0,
                thermal: 0,
                low_power: 0,
                cam_width: self.last_cam_w as u32,
                cam_height: self.last_cam_h as u32,
                cam_bit_depth: self.last_cam_bd as u8,
                cam_matrix: self.last_cam_mx as u8,
                cam_video_range: self.last_cam_vr as u8,
                cam_color_space: self.last_cam_cs as u8,
                cam_poll_submissions_ms: poll_submissions_ms,
                cam_fetch_ms: self.last_cam_fetch_ms,
                cam_setup_ms: preview.setup_ms,
                cam_encode_quad_ms: preview.encode_quad_ms,
                cam_command_buffer_ms: 0.0,
                cam_encoder_ms: 0.0,
                cam_encode_bind_ms: 0.0,
                cam_encode_draw_ms: 0.0,
                cam_end_encoding_ms: 0.0,
                cam_present_ms: preview.present_ms,
                cam_commit_ms: preview.commit_ms,
                cam_gpu_ms: 0.0,
                preview_submission_depth: self.direct_preview_last_submission_depth,
                preview_submission_skipped: self.direct_preview_last_submission_skipped,
                preview_submission_frame_age_ms: self.direct_preview_last_present_frame_age_ms,
                ..PerfStats::default()
            };
            return Ok(self.last_stats);
        }

        let use_live_direct_preview = self.camera_texture_source == CameraTextureSource::Live
            && matches!(
                self.camera_render_mode,
                CameraRenderMode::Nv12Optimized | CameraRenderMode::Nv12Legacy
            )
            && self.sample_count == 1;
        if use_live_direct_preview {
            let vp_dp = [
                (self.target_w as f32) / self.target_scale.max(1.0),
                (self.target_h as f32) / self.target_scale.max(1.0),
            ];
            let rect_dp = [0.0, 0.0, vp_dp[0], vp_dp[1]];
            let fetch_t0 = collect_stage_stats.then(Instant::now);
            with_perf_signpost("camera.renderer.direct.fetch", || {
                self.refresh_live_camera_preview_frame();
            });
            self.last_cam_fetch_ms = elapsed_ms(fetch_t0);
            let current_frame = self.current_live_camera_frame.clone();
            self.direct_preview_backpressure_blocks_present();
            let mut camera_props = current_frame.as_ref().map(|frame| {
                (
                    frame.width,
                    frame.height,
                    frame.bit_depth,
                    frame.matrix,
                    frame.video_range,
                    frame.color_space,
                )
            });
            let mut setup_ms = 0.0;
            let mut encode_quad_ms = 0.0;
            let mut command_buffer_ms = 0.0;
            let mut encoder_ms = 0.0;
            let mut encode_bind_ms = 0.0;
            let mut encode_draw_ms = 0.0;
            let mut end_encoding_ms = 0.0;
            let mut present_ms = 0.0;
            let mut commit_ms = 0.0;
            let mut drew_live_frame = false;
            if self.direct_preview_last_submission_skipped == 0 && !drawable_ptr.is_null() {
                let command_buffer_t0 = collect_stage_stats.then(Instant::now);
                let cmd = with_perf_signpost("camera.renderer.direct.command_buffer", || {
                    self.queue.new_command_buffer().to_owned()
                });
                command_buffer_ms = elapsed_ms(command_buffer_t0);
                let rpd = RenderPassDescriptor::new();
                let setup_t0 = collect_stage_stats.then(Instant::now);
                with_perf_signpost(
                    "camera.renderer.direct.setup",
                    || -> Result<(), api::RenderError> {
                        let raw_drawable_obj = drawable_ptr as *mut Object;
                        let raw_dst_tex: *mut MTLTexture = msg_send![raw_drawable_obj, texture];
                        if raw_dst_tex.is_null() {
                            return Err(api::RenderError::InvalidOperation(
                                "drawable did not provide a destination texture",
                            ));
                        }
                        let ca0 = rpd.color_attachments().object_at(0).unwrap();
                        ca0.set_texture(Some(TextureRef::from_ptr(raw_dst_tex)));
                        ca0.set_store_action(MTLStoreAction::Store);
                        ca0.set_load_action(MTLLoadAction::Clear);
                        ca0.set_clear_color(MTLClearColor {
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            alpha: 1.0,
                        });
                        Ok(())
                    },
                )?;
                setup_ms = elapsed_ms(setup_t0);
                let encoder_t0 = collect_stage_stats.then(Instant::now);
                let enc = with_perf_signpost("camera.renderer.direct.encoder", || {
                    cmd.new_render_command_encoder(&rpd)
                });
                encoder_ms = elapsed_ms(encoder_t0);
                let encode_quad_t0 = collect_stage_stats.then(Instant::now);
                camera_props = with_perf_signpost("camera.renderer.direct.encode_quad", || {
                    current_frame.as_ref().map(|frame| {
                        drew_live_frame = true;
                        let encode_stats = self.encode_camera_quad_from_live_frame(
                            &enc,
                            frame,
                            vp_dp,
                            rect_dp,
                            api::Color::rgba(1.0, 1.0, 1.0, 1.0),
                            1.0,
                            false,
                            collect_stage_stats,
                        );
                        encode_bind_ms = encode_stats.bind_ms;
                        encode_draw_ms = encode_stats.draw_ms;
                        (
                            encode_stats.camera_width,
                            encode_stats.camera_height,
                            encode_stats.camera_bit_depth,
                            encode_stats.camera_matrix,
                            encode_stats.camera_video_range,
                            encode_stats.camera_color_space,
                        )
                    })
                });
                encode_quad_ms = elapsed_ms(encode_quad_t0);
                let end_encoding_t0 = collect_stage_stats.then(Instant::now);
                with_perf_signpost("camera.renderer.direct.end_encoding", || {
                    enc.end_encoding();
                });
                end_encoding_ms = elapsed_ms(end_encoding_t0);
                let raw_drawable = drawable_ptr as *mut MTLDrawable;
                let drawable = DrawableRef::from_ptr(raw_drawable);
                let present_t0 = collect_stage_stats.then(Instant::now);
                with_perf_signpost(
                    "camera.renderer.direct.present_drawable",
                    || -> Result<(), api::RenderError> {
                        cmd.present_drawable(drawable);
                        Ok(())
                    },
                )?;
                self.direct_preview_last_present_frame_age_ms = current_frame
                    .as_ref()
                    .map(|frame| direct_preview_present_frame_age_ms(frame.timestamp_ns))
                    .unwrap_or(0.0);
                present_ms = elapsed_ms(present_t0);
                let commit_t0 = collect_stage_stats.then(Instant::now);
                with_perf_signpost("camera.renderer.direct.commit", || {
                    cmd.commit();
                });
                commit_ms = elapsed_ms(commit_t0);
                self.track_direct_preview_submission(self.frame_id, &cmd);
            }
            if let Some((cw, ch, bd, mx, vr, cs)) = camera_props {
                self.last_cam_w = cw;
                self.last_cam_h = ch;
                self.last_cam_bd = bd;
                self.last_cam_mx = mx;
                self.last_cam_vr = vr;
                self.last_cam_cs = cs;
            } else {
                self.last_cam_w = 0;
                self.last_cam_h = 0;
                self.last_cam_bd = 0;
                self.last_cam_mx = 0;
                self.last_cam_vr = 0;
                self.last_cam_cs = 0;
            }
            self.acc_draws = if drew_live_frame { 1 } else { 0 };
            self.last_stats = PerfStats {
                memory: self.memory_stats(),
                draws: self.acc_draws,
                instanced: self.acc_instanced,
                icb_cmds: self.acc_icb_cmds,
                encode_ms: elapsed_ms(cpu_t0),
                damage_px: 0,
                damage_pct: 0.0,
                damage_rects: 0,
                culled: self.acc_culled,
                blur_ms: 0.0,
                blur_updates: 0,
                blur_period_ms: 0,
                cam_coverage_pct: if drew_live_frame { 1.0 } else { 0.0 },
                cam_paused: 0,
                thermal: 0,
                low_power: 0,
                cam_width: self.last_cam_w.max(0) as u32,
                cam_height: self.last_cam_h.max(0) as u32,
                cam_bit_depth: self.last_cam_bd.max(0) as u8,
                cam_matrix: self.last_cam_mx.max(0) as u8,
                cam_video_range: self.last_cam_vr.max(0) as u8,
                cam_color_space: self.last_cam_cs.max(0) as u8,
                cam_poll_submissions_ms: poll_submissions_ms,
                cam_fetch_ms: self.last_cam_fetch_ms,
                cam_setup_ms: setup_ms,
                cam_encode_quad_ms: encode_quad_ms,
                cam_command_buffer_ms: command_buffer_ms,
                cam_encoder_ms: encoder_ms,
                cam_encode_bind_ms: encode_bind_ms,
                cam_encode_draw_ms: encode_draw_ms,
                cam_end_encoding_ms: end_encoding_ms,
                cam_present_ms: present_ms,
                cam_commit_ms: commit_ms,
                cam_gpu_ms: self.direct_preview_last_completed_gpu_ms,
                preview_submission_depth: self.direct_preview_last_submission_depth,
                preview_submission_skipped: self.direct_preview_last_submission_skipped,
                preview_submission_frame_age_ms: self.direct_preview_last_present_frame_age_ms,
                ..PerfStats::default()
            };
            return Ok(self.last_stats);
        }

        self.ensure_target();

        let raw_direct_tex: *mut MTLTexture = if drawable_ptr.is_null() {
            core::ptr::null_mut()
        } else {
            let raw_drawable_obj = drawable_ptr as *mut Object;
            let raw_tex: *mut MTLTexture = msg_send![raw_drawable_obj, texture];
            if raw_tex.is_null() {
                return Err(api::RenderError::InvalidOperation(
                    "drawable did not provide a destination texture",
                ));
            }
            raw_tex
        };
        let command_buffer_t0 = collect_stage_stats.then(Instant::now);
        let cmd = self.queue.new_command_buffer().to_owned();
        let command_buffer_ms = elapsed_ms(command_buffer_t0);
        let rpd = RenderPassDescriptor::new();
        let setup_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost("camera.renderer.direct.setup", || -> Result<(), api::RenderError> {
            let ca0 = rpd.color_attachments().object_at(0).unwrap();
            if self.sample_count > 1 {
                if let Some(msaa) = &self.target_msaa_tex {
                    ca0.set_texture(Some(msaa));
                } else {
                    return Err(api::RenderError::InvalidOperation(
                        "missing multisample camera preview target",
                    ));
                }
                if !raw_direct_tex.is_null() {
                    ca0.set_resolve_texture(Some(TextureRef::from_ptr(raw_direct_tex)));
                } else if let Some(dst) = &self.target_tex {
                    ca0.set_resolve_texture(Some(dst));
                } else {
                    return Err(api::RenderError::InvalidOperation(
                        "missing camera preview resolve target",
                    ));
                }
                ca0.set_store_action(MTLStoreAction::MultisampleResolve);
            } else {
                if !raw_direct_tex.is_null() {
                    ca0.set_texture(Some(TextureRef::from_ptr(raw_direct_tex)));
                } else if let Some(dst) = &self.target_tex {
                    ca0.set_texture(Some(dst));
                } else {
                    return Err(api::RenderError::InvalidOperation(
                        "missing camera preview target",
                    ));
                }
                ca0.set_store_action(MTLStoreAction::Store);
            }
            ca0.set_load_action(MTLLoadAction::Clear);
            ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
            Ok(())
        })?;
        let setup_ms = elapsed_ms(setup_t0);
        let encoder_t0 = collect_stage_stats.then(Instant::now);
        let enc = cmd.new_render_command_encoder(&rpd);
        let encoder_ms = elapsed_ms(encoder_t0);
        let vp_dp = [
            (self.target_w as f32) / self.target_scale.max(1.0),
            (self.target_h as f32) / self.target_scale.max(1.0),
        ];
        let rect_dp = [0.0, 0.0, vp_dp[0], vp_dp[1]];
        let encode_quad_t0 = collect_stage_stats.then(Instant::now);
        let camera_props = with_perf_signpost("camera.renderer.direct.encode_quad", || {
            self.encode_camera_quad(
                &enc,
                vp_dp,
                rect_dp,
                api::Color::rgba(1.0, 1.0, 1.0, 1.0),
                1.0,
                false,
            )
        });
        let encode_quad_ms = elapsed_ms(encode_quad_t0);
        if let Some((cw, ch, bd, mx, vr, cs)) = camera_props {
            self.last_cam_w = cw;
            self.last_cam_h = ch;
            self.last_cam_bd = bd;
            self.last_cam_mx = mx;
            self.last_cam_vr = vr;
            self.last_cam_cs = cs;
            self.acc_draws = 1;
        } else {
            self.last_cam_w = 0;
            self.last_cam_h = 0;
            self.last_cam_bd = 0;
            self.last_cam_mx = 0;
            self.last_cam_vr = 0;
            self.last_cam_cs = 0;
        }
        let end_encoding_t0 = collect_stage_stats.then(Instant::now);
        let end_encoding_t0 = collect_stage_stats.then(Instant::now);
        enc.end_encoding();
        let end_encoding_ms = elapsed_ms(end_encoding_t0);
        let end_encoding_ms = elapsed_ms(end_encoding_t0);

        let mut present_ms = 0.0;
        if !drawable_ptr.is_null() {
            let raw_drawable = drawable_ptr as *mut MTLDrawable;
            let drawable = DrawableRef::from_ptr(raw_drawable);
            let present_t0 = collect_stage_stats.then(Instant::now);
            with_perf_signpost("camera.renderer.direct.present_drawable", || {
                cmd.present_drawable(drawable);
            });
            present_ms = elapsed_ms(present_t0);
        }
        let commit_t0 = collect_stage_stats.then(Instant::now);
        with_perf_signpost("camera.renderer.direct.commit", || {
            cmd.commit();
        });
        let commit_ms = elapsed_ms(commit_t0);
        self.track_direct_preview_submission(self.frame_id, &cmd);
        self.last_stats = PerfStats {
            memory: self.memory_stats(),
            draws: self.acc_draws,
            instanced: self.acc_instanced,
            icb_cmds: self.acc_icb_cmds,
            encode_ms: elapsed_ms(cpu_t0),
            damage_px: 0,
            damage_pct: 0.0,
            damage_rects: 0,
            culled: self.acc_culled,
            blur_ms: 0.0,
            blur_updates: 0,
            blur_period_ms: 0,
            cam_coverage_pct: if camera_props.is_some() { 1.0 } else { 0.0 },
            cam_paused: 0,
            thermal: 0,
            low_power: 0,
            cam_width: self.last_cam_w.max(0) as u32,
            cam_height: self.last_cam_h.max(0) as u32,
            cam_bit_depth: self.last_cam_bd.max(0) as u8,
            cam_matrix: self.last_cam_mx.max(0) as u8,
            cam_video_range: self.last_cam_vr.max(0) as u8,
            cam_color_space: self.last_cam_cs.max(0) as u8,
            cam_poll_submissions_ms: poll_submissions_ms,
            cam_fetch_ms: self.last_cam_fetch_ms,
            cam_setup_ms: setup_ms,
            cam_encode_quad_ms: encode_quad_ms,
            cam_command_buffer_ms: command_buffer_ms,
            cam_encoder_ms: encoder_ms,
            cam_encode_bind_ms: 0.0,
            cam_encode_draw_ms: 0.0,
            cam_end_encoding_ms: end_encoding_ms,
            cam_present_ms: present_ms,
            cam_commit_ms: commit_ms,
            cam_gpu_ms: self.direct_preview_last_completed_gpu_ms,
            preview_submission_depth: self.direct_preview_last_submission_depth,
            preview_submission_skipped: self.direct_preview_last_submission_skipped,
            preview_submission_frame_age_ms: self.direct_preview_last_present_frame_age_ms,
            ..PerfStats::default()
        };
        Ok(self.last_stats)
    }

    pub fn readback_bgra8(&mut self) -> Option<(u32, u32, alloc::vec::Vec<u8>)> {
        if self.color_format != MTLPixelFormat::BGRA8Unorm_sRGB {
            return None;
        }
        let tex = self.target_tex.as_ref()?;
        let w = tex.width() as u32;
        let h = tex.height() as u32;
        let row_bytes = (w as usize) * 4;
        let buf_bytes = row_bytes * (h as usize);
        let opts =
            MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeShared;
        let buf = self.device.new_buffer(buf_bytes as u64, opts);
        let cmd = self.queue.new_command_buffer();
        let blit = cmd.new_blit_command_encoder();
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let size = MTLSize { width: w as u64, height: h as u64, depth: 1 };
        blit.copy_from_texture_to_buffer(
            tex,
            0,
            0,
            origin,
            size,
            &buf,
            0,
            row_bytes as u64,
            (row_bytes * (h as usize)) as u64,
            MTLBlitOption::empty(),
        );
        blit.end_encoding();
        cmd.commit();
        cmd.wait_until_completed();
        let ptr = buf.contents();
        if ptr.is_null() {
            return None;
        }
        let out = unsafe { core::slice::from_raw_parts(ptr as *const u8, buf_bytes) };
        Some((w, h, out.to_vec()))
    }
}

// Build a filtered copy of a DrawList that keeps only items whose bounding
// rect (in dp) intersects the provided dp scissor. Vertices/indices are
// copied by reference (cloned arrays), spans remain valid.
fn filter_drawlist_by_dp_scissor(list: &api::DrawList, sc: api::RectI) -> api::DrawList {
    fn rect_intersects(r: &api::RectF, sc: &api::RectI) -> bool {
        let rx0 = r.x;
        let ry0 = r.y;
        let rx1 = r.x + r.w;
        let ry1 = r.y + r.h;
        let sx0 = sc.x as f32;
        let sy0 = sc.y as f32;
        let sx1 = (sc.x + sc.w) as f32;
        let sy1 = (sc.y + sc.h) as f32;
        rx1 > sx0 && rx0 < sx1 && ry1 > sy0 && ry0 < sy1
    }
    let mut out = api::DrawList {
        items: alloc::vec::Vec::new(),
        vertices: list.vertices.clone(),
        indices: list.indices.clone(),
    };
    let mut i = 0usize;
    while i < list.items.len() {
        match &list.items[i] {
            api::DrawCmd::RRect { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::CameraBg { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::NineSlice { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::Image { dst, .. } => {
                if rect_intersects(dst, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::Spinner { center, atom, .. } => {
                let rect = api::RectF {
                    x: center[0] - atom * 0.5,
                    y: center[1] - atom * 0.5,
                    w: *atom,
                    h: *atom,
                };
                if rect_intersects(&rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::Backdrop { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::GlyphRun { run } => {
                // Compute bounding box from vertices
                let v_count = run.vb.len as usize;
                if v_count == 0 {
                    i += 1;
                    continue;
                }
                let srcv =
                    &list.vertices[(run.vb.offset as usize)..(run.vb.offset as usize + v_count)];
                let mut minx = f32::INFINITY;
                let mut miny = f32::INFINITY;
                let mut maxx = f32::NEG_INFINITY;
                let mut maxy = f32::NEG_INFINITY;
                for v in srcv.iter() {
                    minx = minx.min(v.x);
                    miny = miny.min(v.y);
                    maxx = maxx.max(v.x);
                    maxy = maxy.max(v.y);
                }
                let rect = api::RectF {
                    x: minx,
                    y: miny,
                    w: (maxx - minx).max(0.0),
                    h: (maxy - miny).max(0.0),
                };
                if rect_intersects(&rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::LayerBegin { rect, .. } => {
                // If layer rect doesn't intersect, skip until matching LayerEnd
                let mut depth = 1usize;
                let mut j = i + 1;
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                    while j < list.items.len() && depth > 0 {
                        match &list.items[j] {
                            api::DrawCmd::LayerBegin { .. } => {
                                depth += 1;
                                out.items.push(list.items[j].clone());
                            }
                            api::DrawCmd::LayerEnd => {
                                depth -= 1;
                                out.items.push(list.items[j].clone());
                            }
                            _ => out.items.push(list.items[j].clone()),
                        }
                        j += 1;
                    }
                } else {
                    while j < list.items.len() && depth > 0 {
                        match &list.items[j] {
                            api::DrawCmd::LayerBegin { .. } => depth += 1,
                            api::DrawCmd::LayerEnd => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                }
                i = j;
            }
            api::DrawCmd::LayerEnd => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
            api::DrawCmd::Solid { .. } => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
            api::DrawCmd::ClipPush { .. } | api::DrawCmd::ClipPop => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
        }
    }
    out
}

impl Drop for MetalRenderer {
    fn drop(&mut self) {
        self.release_live_camera_frame();
    }
}

impl api::Renderer for MetalRenderer {
    fn device_caps(&self) -> api::DeviceCaps {
        api::DeviceCaps {
            max_framerate_hz: 120,
            supports_edr: self.hdr_enabled,
            supports_msaa4x: self.sample_count >= 4,
            native_scale: 1.0,
        }
    }

    fn begin_frame(
        &mut self,
        _fb: &api::FrameTarget,
        damage: Option<&api::Damage>,
    ) -> api::FrameToken {
        self.frame_id = self.frame_id.wrapping_add(1);
        let slot = (self.frame_id % FRAME_RING_SIZE as u64) as usize;
        self.frames[slot].prepare_for_encode();
        self.acc_draws = 0;
        self.acc_instanced = 0;
        self.acc_icb_cmds = 0;
        self.acc_culled = 0;
        // Defer command buffer creation to encode_pass
        self.frames[slot].cmd = None;
        // Reset per-frame accumulators
        self.scissor_changes = 0;
        self.prepass_shaded_px = 0;
        self.main_shaded_px = 0;
        // Capture frame-level scissor in dp when enabled
        if self.damage_enabled {
            if let Some(d) = damage {
                self.frame_damage_rects = d.rects.len() as u32;
                // Union of provided rects (dp)
                let mut it = d.rects.iter();
                if let Some(first) = it.next() {
                    let mut x0 = first.x;
                    let mut y0 = first.y;
                    let mut x1 = first.x + first.w;
                    let mut y1 = first.y + first.h;
                    for r in it {
                        x0 = x0.min(r.x);
                        y0 = y0.min(r.y);
                        x1 = x1.max(r.x + r.w);
                        y1 = y1.max(r.y + r.h);
                    }
                    let w = (x1 - x0).max(0);
                    let h = (y1 - y0).max(0);
                    if w > 0 && h > 0 {
                        self.frame_scissor_dp = Some(api::RectI { x: x0, y: y0, w, h });
                    } else {
                        self.frame_scissor_dp = None;
                    }
                } else {
                    self.frame_scissor_dp = None;
                }
            } else {
                self.frame_scissor_dp = None;
                self.frame_damage_rects = 0;
            }
        } else {
            self.frame_scissor_dp = None;
            self.frame_damage_rects = 0;
        }
        // Compute damage coverage metrics
        if let Some(dp) = self.frame_scissor_dp {
            let vp_w_dp = (self.target_w as f32) / self.target_scale.max(1.0);
            let vp_h_dp = (self.target_h as f32) / self.target_scale.max(1.0);
            let vp_area_dp = (vp_w_dp.max(1.0)) * (vp_h_dp.max(1.0));
            let dmg_area_dp = (dp.w.max(0) as f32) * (dp.h.max(0) as f32);
            self.frame_damage_pct =
                if vp_area_dp > 0.0 { (dmg_area_dp / vp_area_dp).clamp(0.0, 1.0) } else { 0.0 };
            // Convert to px and clamp to framebuffer bounds
            let s = self.target_scale.max(1.0);
            let x = (dp.x as f32 * s).floor() as i32;
            let y = (dp.y as f32 * s).floor() as i32;
            let w = (dp.w as f32 * s).ceil() as i32;
            let h = (dp.h as f32 * s).ceil() as i32;
            let tx = 0;
            let ty = 0;
            let tw = self.target_w as i32;
            let th = self.target_h as i32;
            let x1 = x.clamp(tx, tx + tw);
            let y1 = y.clamp(ty, ty + th);
            let x2 = (x + w).clamp(tx, tx + tw);
            let y2 = (y + h).clamp(ty, ty + th);
            let rw = (x2 - x1).max(0) as u64;
            let rh = (y2 - y1).max(0) as u64;
            self.frame_damage_px = rw.saturating_mul(rh);
        } else {
            self.frame_damage_pct = 0.0;
            self.frame_damage_px = 0;
        }
        api::FrameToken(self.frame_id)
    }

    fn encode_pass(&mut self, list: &api::DrawList) {
        let cpu_t0 = std::time::Instant::now();
        let direct_present = self.pending_present_texture != 0;
        if self.target_tex.is_none() && !direct_present {
            return;
        }
        if self.submit_error_flag.load(Ordering::Acquire) {
            if ios_log_enabled() {
                ios_log("oxide.renderer-metal: skipping encode_pass due pending submit error");
            }
            return;
        }
        let slot = (self.frame_id % FRAME_RING_SIZE as u64) as usize;
        // Create command buffer for this frame now
        let cmd = self.queue.new_command_buffer().to_owned();
        self.frames[slot].cmd = Some(cmd.to_owned());

        // Adaptive policy: compute camera coverage and environment (iOS thermal/LPM),
        // then tune blur update period and optionally pause camera when hot with tiny coverage.
        let vp_w_dp = (self.target_w as f32) / self.target_scale.max(1.0);
        let vp_h_dp = (self.target_h as f32) / self.target_scale.max(1.0);
        let vp_area_dp = (vp_w_dp.max(1.0)) * (vp_h_dp.max(1.0));
        let mut cam_area: f32 = 0.0;
        for it in &list.items {
            if let api::DrawCmd::CameraBg { rect, .. } = it {
                let a = (rect.w.max(0.0) * rect.h.max(0.0)).min(vp_area_dp);
                cam_area += a;
            }
        }
        let cam_coverage =
            if vp_area_dp > 0.0 { (cam_area / vp_area_dp).clamp(0.0, 1.0) } else { 0.0 };
        #[cfg(target_os = "ios")]
        let (lpm, therm) = unsafe {
            extern "C" {
                fn oxide_host_power_lowpower() -> ::libc::c_int;
                fn oxide_host_thermal_state() -> ::libc::c_int;
            }
            (oxide_host_power_lowpower() != 0, oxide_host_thermal_state())
        };
        #[cfg(not(target_os = "ios"))]
        let (lpm, therm) = (false, 0);
        // Tune blur update period
        let mut period_ms: u64 = 83; // ~12 fps
        if lpm || therm >= 2 {
            period_ms = 120;
        } else if therm == 1 {
            period_ms = 100;
        }
        if cam_coverage < 0.15 {
            period_ms = period_ms.max(110);
        }
        if self.cam_update_period != std::time::Duration::from_millis(period_ms) {
            self.cam_update_period = std::time::Duration::from_millis(period_ms);
        }
        // Pause/resume capture when very hot and tiny coverage to save power
        #[cfg(target_os = "ios")]
        unsafe {
            extern "C" {
                fn oxide_cam_stop();
                fn oxide_cam_start_default();
            }
            if (lpm || therm >= 2) && cam_coverage < 0.05 {
                self.cam_pause_frames = self.cam_pause_frames.saturating_add(1);
                if self.cam_pause_frames > 30 && !self.cam_paused {
                    oxide_cam_stop();
                    self.cam_paused = true;
                }
            } else {
                self.cam_pause_frames = 0;
                if self.cam_paused && cam_coverage > 0.10 {
                    oxide_cam_start_default();
                    self.cam_paused = false;
                }
            }
        }

        // Camera blur prepass: if any CameraBg requests blur, update a cached blurred camera
        let need_cam_blur =
            list.items.iter().any(|c| matches!(c, api::DrawCmd::CameraBg { blur: true, .. }));
        #[cfg(target_os = "ios")]
        let mut blur_ms_out: f64 = 0.0;
        #[cfg(not(target_os = "ios"))]
        let mut blur_ms_out: f64 = 0.0;
        #[cfg(target_os = "ios")]
        let mut blur_updated: u32 = 0;
        #[cfg(not(target_os = "ios"))]
        let mut blur_updated: u32 = 0;
        if need_cam_blur {
            let do_update = match self.cam_last_update {
                None => true,
                Some(t) => t.elapsed() >= self.cam_update_period,
            };
            if do_update {
                let blur_t0 = std::time::Instant::now();
                let now = std::time::Instant::now();
                let vp_dp: [f32; 2] = [
                    (self.target_w as f32) / self.target_scale.max(1.0),
                    (self.target_h as f32) / self.target_scale.max(1.0),
                ];
                let rect_dp: [f32; 4] = [0.0, 0.0, vp_dp[0], vp_dp[1]];
                self.ensure_effect_targets();
                if let Some(src) = &self.prepass_tex {
                    let rpd0 = RenderPassDescriptor::new();
                    let ca = rpd0.color_attachments().object_at(0).unwrap();
                    ca.set_texture(Some(src));
                    ca.set_load_action(MTLLoadAction::Clear);
                    ca.set_clear_color(MTLClearColor {
                        red: 0.0,
                        green: 0.0,
                        blue: 0.0,
                        alpha: 1.0,
                    });
                    ca.set_store_action(MTLStoreAction::Store);
                    let enc0 = cmd.new_render_command_encoder(&rpd0);
                    if let Some((cw, ch, bd, mx, vr, cs)) = self.encode_camera_quad(
                        &enc0,
                        vp_dp,
                        rect_dp,
                        api::Color::rgba(1.0, 1.0, 1.0, 1.0),
                        1.0,
                        false,
                    ) {
                        let changed = self.last_cam_w != cw
                            || self.last_cam_h != ch
                            || self.last_cam_bd != bd
                            || self.last_cam_mx != mx
                            || self.last_cam_vr != vr
                            || self.last_cam_cs != cs;
                        if changed {
                            if let Some(tex) = &self.cam_blur_tex {
                                self.cam_xfade_prev_tex = Some(tex.to_owned());
                                self.cam_xfade_t0 = Some(now);
                            }
                            self.last_cam_w = cw;
                            self.last_cam_h = ch;
                            self.last_cam_bd = bd;
                            self.last_cam_mx = mx;
                            self.last_cam_vr = vr;
                            self.last_cam_cs = cs;
                        }
                        if self.cam_blur_tex.is_none() {
                            self.cam_blur_fade_t0 = Some(now);
                        }
                    }
                    enc0.end_encoding();
                }
                if let (Some(pre), Some(half)) = (&self.prepass_tex, &self.half_tex) {
                    let rpd = RenderPassDescriptor::new();
                    let ca = rpd.color_attachments().object_at(0).unwrap();
                    ca.set_texture(Some(half));
                    ca.set_load_action(MTLLoadAction::DontCare);
                    ca.set_store_action(MTLStoreAction::Store);
                    let enc = cmd.new_render_command_encoder(&rpd);
                    enc.set_render_pipeline_state(&self.pso_downsample);
                    if let Some(sam) = &self.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(pre));
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&rect_dp) as u64,
                        rect_dp.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                    enc.end_encoding();
                }
                if let (Some(half), Some(q)) = (&self.half_tex, &self.quarter_tex) {
                    let rpd = RenderPassDescriptor::new();
                    let ca = rpd.color_attachments().object_at(0).unwrap();
                    ca.set_texture(Some(q));
                    ca.set_load_action(MTLLoadAction::DontCare);
                    ca.set_store_action(MTLStoreAction::Store);
                    let enc = cmd.new_render_command_encoder(&rpd);
                    enc.set_render_pipeline_state(&self.pso_downsample);
                    if let Some(sam) = &self.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(half));
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&rect_dp) as u64,
                        rect_dp.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                    enc.end_encoding();
                }
                if let (Some(q), Some(qtmp)) = (&self.quarter_tex, &self.quarter_tmp_tex) {
                    let rpd = RenderPassDescriptor::new();
                    let ca = rpd.color_attachments().object_at(0).unwrap();
                    ca.set_texture(Some(qtmp));
                    ca.set_load_action(MTLLoadAction::DontCare);
                    ca.set_store_action(MTLStoreAction::Store);
                    let enc = cmd.new_render_command_encoder(&rpd);
                    enc.set_render_pipeline_state(&self.pso_blur);
                    if let Some(sam) = &self.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(q));
                    let params_h: [f32; 4] = [1.0, 0.0, 6.0, 0.0];
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&rect_dp) as u64,
                        rect_dp.as_ptr() as *const _,
                    );
                    enc.set_fragment_bytes(
                        1,
                        core::mem::size_of_val(&params_h) as u64,
                        params_h.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                    enc.end_encoding();
                    let rpd2 = RenderPassDescriptor::new();
                    let ca2 = rpd2.color_attachments().object_at(0).unwrap();
                    ca2.set_texture(Some(q));
                    ca2.set_load_action(MTLLoadAction::DontCare);
                    ca2.set_store_action(MTLStoreAction::Store);
                    let enc2 = cmd.new_render_command_encoder(&rpd2);
                    enc2.set_render_pipeline_state(&self.pso_blur);
                    if let Some(sam) = &self.sampler {
                        enc2.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc2.set_fragment_texture(0, Some(qtmp));
                    let params_v: [f32; 4] = [0.0, 1.0, 6.0, 0.0];
                    enc2.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc2.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&rect_dp) as u64,
                        rect_dp.as_ptr() as *const _,
                    );
                    enc2.set_fragment_bytes(
                        1,
                        core::mem::size_of_val(&params_v) as u64,
                        params_v.as_ptr() as *const _,
                    );
                    enc2.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                    enc2.end_encoding();
                    self.cam_blur_tex = Some(q.to_owned());
                }
                self.cam_last_update = Some(std::time::Instant::now());
                blur_ms_out = blur_t0.elapsed().as_secs_f64() * 1000.0;
                blur_updated = 1;
            }
        }

        // Pre-render cacheable layers into textures.
        // Simulator defaults this off for correctness; layers are then rendered inline.
        if self.layer_cache_enabled {
            let mut i = 0usize;
            while i < list.items.len() {
                if let api::DrawCmd::LayerBegin { id, rect, dirty } = &list.items[i] {
                    // find end
                    let mut depth = 1usize;
                    let mut j = i + 1;
                    let mut unsupported = false;
                    while j < list.items.len() && depth > 0 {
                        match &list.items[j] {
                            api::DrawCmd::LayerBegin { .. } => depth += 1,
                            api::DrawCmd::LayerEnd => depth -= 1,
                            api::DrawCmd::Solid { .. } | api::DrawCmd::Backdrop { .. } => {
                                unsupported = true
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    let end = j - 1;
                    if !unsupported {
                        // Build offset sublist like in encode_draws
                        let ox = rect.x;
                        let oy = rect.y;
                        let mut sub = api::DrawList {
                            items: alloc::vec::Vec::new(),
                            vertices: alloc::vec::Vec::new(),
                            indices: alloc::vec::Vec::new(),
                        };
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        for k in i + 1..end {
                            match &list.items[k] {
                                api::DrawCmd::ClipPush { rect: r0 } => {
                                    let mut rr = *r0;
                                    rr.x -= ox as i32;
                                    rr.y -= oy as i32;
                                    sub.items.push(api::DrawCmd::ClipPush { rect: rr });
                                }
                                api::DrawCmd::ClipPop => sub.items.push(api::DrawCmd::ClipPop),
                                api::DrawCmd::RRect { rect: r0, radii, color } => {
                                    let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                                    sub.items.push(api::DrawCmd::RRect {
                                        rect: adj,
                                        radii: *radii,
                                        color: *color,
                                    });
                                }
                                api::DrawCmd::NineSlice { tex, rect: r0, slice, alpha } => {
                                    let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                                    sub.items.push(api::DrawCmd::NineSlice {
                                        tex: *tex,
                                        rect: adj,
                                        slice: *slice,
                                        alpha: *alpha,
                                    });
                                }
                                api::DrawCmd::Image { tex, dst, src, alpha } => {
                                    let adj = api::RectF::new(dst.x - ox, dst.y - oy, dst.w, dst.h);
                                    sub.items.push(api::DrawCmd::Image {
                                        tex: *tex,
                                        dst: adj,
                                        src: *src,
                                        alpha: *alpha,
                                    });
                                }
                                api::DrawCmd::Spinner { center, atom, alpha } => {
                                    let adj = [center[0] - ox, center[1] - oy];
                                    sub.items.push(api::DrawCmd::Spinner {
                                        center: adj,
                                        atom: *atom,
                                        alpha: *alpha,
                                    });
                                }
                                api::DrawCmd::GlyphRun { run } => {
                                    let v_count = run.vb.len as usize;
                                    let i_count = run.ib.len as usize;
                                    let new_v_off = sub.vertices.len() as u32;
                                    let srcv = &list.vertices[(run.vb.offset as usize)
                                        ..(run.vb.offset as usize + v_count)];
                                    for v in srcv.iter() {
                                        let mut vv = *v;
                                        vv.x -= ox;
                                        vv.y -= oy;
                                        sub.vertices.push(vv);
                                    }
                                    let srci = &list.indices[(run.ib.offset as usize)
                                        ..(run.ib.offset as usize + i_count)];
                                    let Some(remapped_indices) = remap_indices_to_span(
                                        srci,
                                        run.vb.offset,
                                        run.vb.len,
                                        new_v_off,
                                    ) else {
                                        continue;
                                    };
                                    sub.indices.extend_from_slice(&remapped_indices);
                                    sub.items.push(api::DrawCmd::GlyphRun {
                                        run: api::GlyphRun {
                                            atlas: run.atlas,
                                            vb: api::VertexSpan {
                                                offset: new_v_off,
                                                len: v_count as u32,
                                            },
                                            ib: api::IndexSpan {
                                                offset: (sub.indices.len() as u32)
                                                    .wrapping_sub(i_count as u32),
                                                len: i_count as u32,
                                            },
                                            sdf: run.sdf,
                                            color: run.color,
                                        },
                                    });
                                }
                                _ => {}
                            }
                        }
                        // Hash: use number of items and vertex count
                        use std::hash::Hash;
                        (sub.items.len() as u64).hash(&mut hasher);
                        (sub.vertices.len() as u64).hash(&mut hasher);
                        let hash = hasher.finish();
                        let w_px = (rect.w * self.target_scale.max(1.0)).ceil() as u32;
                        let h_px = (rect.h * self.target_scale.max(1.0)).ceil() as u32;
                        let need = *dirty
                            || !self.layers.get(id).is_some()
                            || self
                                .layers
                                .get(id)
                                .map(|e| e.w != w_px || e.h != h_px || e.hash != hash)
                                .unwrap_or(true);
                        if need {
                            let d = TextureDescriptor::new();
                            d.set_pixel_format(self.color_format);
                            d.set_texture_type(MTLTextureType::D2);
                            d.set_width(w_px as u64);
                            d.set_height(h_px as u64);
                            d.set_storage_mode(MTLStorageMode::Private);
                            d.set_usage(
                                MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
                            );
                            let tex = self.device.new_texture(&d);
                            let rpdl = RenderPassDescriptor::new();
                            let ca_l = rpdl.color_attachments().object_at(0).unwrap();
                            ca_l.set_texture(Some(&tex));
                            ca_l.set_load_action(MTLLoadAction::Clear);
                            ca_l.set_clear_color(MTLClearColor {
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                                alpha: 0.0,
                            });
                            ca_l.set_store_action(MTLStoreAction::Store);
                            let encl = cmd.new_render_command_encoder(&rpdl);
                            let mut pf_l = PerFrame::new();
                            // Temporarily change viewport values
                            let old_w = self.target_w;
                            let old_h = self.target_h;
                            let old_scale = self.target_scale;
                            self.target_w = w_px;
                            self.target_h = h_px;
                            self.target_scale = old_scale;
                            encode_draws(&encl, &mut pf_l, self, &sub, false, None);
                            self.target_w = old_w;
                            self.target_h = old_h;
                            self.target_scale = old_scale;
                            encl.end_encoding();
                            self.layers.insert(*id, LayerEntry { tex, w: w_px, h: h_px, hash });
                        }
                    }
                    i = end + 1;
                    continue;
                }
                i += 1;
            }
        }

        // Effects prepass: if there is any Backdrop, render a prepass and blur it.
        let has_backdrop = list.items.iter().any(|c| matches!(c, api::DrawCmd::Backdrop { .. }));
        if has_backdrop {
            self.ensure_effect_targets();
            // 1) Prepass: render up to the first Backdrop into prepass_tex
            let rpd0 = RenderPassDescriptor::new();
            let ca_pre = rpd0.color_attachments().object_at(0).unwrap();
            if let Some(src) = &self.prepass_tex {
                ca_pre.set_texture(Some(src));
            }
            ca_pre.set_load_action(MTLLoadAction::Clear);
            ca_pre.set_clear_color(MTLClearColor { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 });
            ca_pre.set_store_action(MTLStoreAction::Store);
            let enc0 = cmd.new_render_command_encoder(&rpd0);
            // Move out per-frame to avoid double-borrow
            let mut pf0 = core::mem::take(&mut self.frames[slot]);
            // Compute prepass scissor: union of Backdrop rects (expanded) intersect frame scissor if enabled
            let mut prepass_scissor_dp: Option<api::RectI> = None;
            {
                let mut sigma = 6.0f32;
                let s = self.target_scale.max(1.0);
                let mut x0 = self.target_w as i32;
                let mut y0 = self.target_h as i32;
                let mut x1 = 0i32;
                let mut y1 = 0i32;
                let mut found_any = false;
                for c in &list.items {
                    if let api::DrawCmd::Backdrop { rect, sigma: sg, .. } = c {
                        if *sg > sigma {
                            sigma = *sg;
                        }
                        let margin = (3.0 * *sg).ceil();
                        let rx0 = (rect.x - margin).floor() as i32;
                        let ry0 = (rect.y - margin).floor() as i32;
                        let rx1 = (rect.x + rect.w + margin).ceil() as i32;
                        let ry1 = (rect.y + rect.h + margin).ceil() as i32;
                        x0 = x0.min(rx0);
                        y0 = y0.min(ry0);
                        x1 = x1.max(rx1);
                        y1 = y1.max(ry1);
                        found_any = true;
                    }
                }
                if found_any {
                    // Clamp to framebuffer dp bounds
                    let x0c = x0.clamp(0, (self.target_w as f32 / s) as i32);
                    let y0c = y0.clamp(0, (self.target_h as f32 / s) as i32);
                    let x1c = x1.clamp(0, (self.target_w as f32 / s) as i32);
                    let y1c = y1.clamp(0, (self.target_h as f32 / s) as i32);
                    let rx = x0c.max(0);
                    let ry = y0c.max(0);
                    let rw = (x1c - x0c).max(0);
                    let rh = (y1c - y0c).max(0);
                    let mut rect = api::RectI { x: rx, y: ry, w: rw, h: rh };
                    // Intersect with frame damage scissor if enabled
                    if self.damage_enabled {
                        if let Some(g) = self.frame_scissor_dp {
                            // intersect dp
                            let ix0 = rect.x.max(g.x);
                            let iy0 = rect.y.max(g.y);
                            let ix1 = (rect.x + rect.w).min(g.x + g.w);
                            let iy1 = (rect.y + rect.h).min(g.y + g.h);
                            let iw = (ix1 - ix0).max(0);
                            let ih = (iy1 - iy0).max(0);
                            rect = api::RectI { x: ix0, y: iy0, w: iw, h: ih };
                        }
                    }
                    if rect.w > 0 && rect.h > 0 {
                        prepass_scissor_dp = Some(rect);
                    }
                }
            }
            // Heuristics: drop prepass scissor when damage coverage is large
            let dmg_thresh: f32 = self.damage_use_thresh;
            if prepass_scissor_dp.is_some() && self.frame_damage_pct >= dmg_thresh {
                prepass_scissor_dp = None;
            }
            // Optional pre-filtering by prepass scissor only when damage is small
            let filtered_prepass;
            let list_pre_ref = if let Some(sc_dp) = prepass_scissor_dp {
                if self.frame_damage_pct <= self.damage_prefilter_thresh {
                    filtered_prepass = filter_drawlist_by_dp_scissor(list, sc_dp);
                    if filtered_prepass.items.len() < list.items.len() {
                        self.acc_culled = self.acc_culled.saturating_add(
                            (list.items.len() - filtered_prepass.items.len()) as u32,
                        );
                    }
                    &filtered_prepass
                } else {
                    list
                }
            } else {
                list
            };
            encode_draws(&enc0, &mut pf0, self, list_pre_ref, true, prepass_scissor_dp);
            self.frames[slot] = pf0;
            enc0.end_encoding();

            // Determine blur kernel and union scissor in pixel coords for all Backdrop rects
            let mut sigma = 6.0f32;
            let mut u_x0: i32 = self.target_w as i32;
            let mut u_y0: i32 = self.target_h as i32;
            let mut u_x1: i32 = 0;
            let mut u_y1: i32 = 0;
            let scale = self.target_scale.max(1.0);
            let mut found_any = false;
            for c in &list.items {
                if let api::DrawCmd::Backdrop { rect, sigma: s, .. } = c {
                    if *s > sigma {
                        sigma = *s;
                    }
                    // Expand by ~3*sigma kernel radius, convert to px then clamp
                    let margin = (3.0 * *s).ceil();
                    let x0 = ((rect.x - margin) * scale).floor() as i32;
                    let y0 = ((rect.y - margin) * scale).floor() as i32;
                    let x1 = ((rect.x + rect.w + margin) * scale).ceil() as i32;
                    let y1 = ((rect.y + rect.h + margin) * scale).ceil() as i32;
                    u_x0 = u_x0.min(x0);
                    u_y0 = u_y0.min(y0);
                    u_x1 = u_x1.max(x1);
                    u_y1 = u_y1.max(y1);
                    found_any = true;
                }
            }
            if !found_any {
                sigma = 6.0;
                u_x0 = 0;
                u_y0 = 0;
                u_x1 = self.target_w as i32;
                u_y1 = self.target_h as i32;
            }
            // Clamp to framebuffer bounds and ensure non-negative width/height
            let x0c = u_x0.clamp(0, self.target_w as i32);
            let y0c = u_y0.clamp(0, self.target_h as i32);
            let x1c = u_x1.clamp(0, self.target_w as i32);
            let y1c = u_y1.clamp(0, self.target_h as i32);
            let sc_x = x0c.max(0) as u64;
            let sc_y = y0c.max(0) as u64;
            let sc_w = (x1c - x0c).max(0) as u64;
            let sc_h = (y1c - y0c).max(0) as u64;

            // 2) Downsample: prepass_tex -> half_tex -> quarter_tex
            let sc_half = MTLScissorRect {
                x: sc_x / 2,
                y: sc_y / 2,
                width: (sc_w / 2).max(0),
                height: (sc_h / 2).max(0),
            };
            let sc_quarter = MTLScissorRect {
                x: sc_x / 4,
                y: sc_y / 4,
                width: (sc_w / 4).max(0),
                height: (sc_h / 4).max(0),
            };

            // prepass -> half
            let rpd_ds1 = RenderPassDescriptor::new();
            let ca_ds1 = rpd_ds1.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.half_tex {
                ca_ds1.set_texture(Some(dst));
            }
            ca_ds1.set_load_action(MTLLoadAction::DontCare);
            ca_ds1.set_store_action(MTLStoreAction::Store);
            let enc_ds1 = cmd.new_render_command_encoder(&rpd_ds1);
            enc_ds1.set_render_pipeline_state(&self.pso_downsample);
            if let Some(sam) = &self.sampler {
                enc_ds1.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.prepass_tex {
                enc_ds1.set_fragment_texture(0, Some(src));
            }
            enc_ds1.set_scissor_rect(sc_half);
            enc_ds1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px =
                self.prepass_shaded_px.saturating_add(sc_half.width.saturating_mul(sc_half.height));
            enc_ds1.end_encoding();

            // half -> quarter
            let rpd_ds2 = RenderPassDescriptor::new();
            let ca_ds2 = rpd_ds2.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.quarter_tex {
                ca_ds2.set_texture(Some(dst));
            }
            ca_ds2.set_load_action(MTLLoadAction::DontCare);
            ca_ds2.set_store_action(MTLStoreAction::Store);
            let enc_ds2 = cmd.new_render_command_encoder(&rpd_ds2);
            enc_ds2.set_render_pipeline_state(&self.pso_downsample);
            if let Some(sam) = &self.sampler {
                enc_ds2.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.half_tex {
                enc_ds2.set_fragment_texture(0, Some(src));
            }
            enc_ds2.set_scissor_rect(sc_quarter);
            enc_ds2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px = self
                .prepass_shaded_px
                .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
            enc_ds2.end_encoding();

            // 3) Blur H at quarter: quarter -> quarter_tmp
            let rpd1 = RenderPassDescriptor::new();
            let ca_blur_h = rpd1.color_attachments().object_at(0).unwrap();
            if let Some(tmp) = &self.quarter_tmp_tex {
                ca_blur_h.set_texture(Some(tmp));
            }
            ca_blur_h.set_load_action(MTLLoadAction::DontCare);
            ca_blur_h.set_store_action(MTLStoreAction::Store);
            let enc1 = cmd.new_render_command_encoder(&rpd1);
            enc1.set_render_pipeline_state(&self.pso_blur);
            if let Some(sam) = &self.sampler {
                enc1.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.quarter_tex {
                enc1.set_fragment_texture(0, Some(src));
            }
            enc1.set_scissor_rect(sc_quarter);
            let params_h: [f32; 4] = [1.0, 0.0, sigma / 4.0, 0.0];
            enc1.set_fragment_bytes(
                1,
                core::mem::size_of_val(&params_h) as u64,
                params_h.as_ptr() as *const _,
            );
            enc1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px = self
                .prepass_shaded_px
                .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
            enc1.end_encoding();

            // 4) Blur V at quarter: quarter_tmp -> quarter
            let rpd2 = RenderPassDescriptor::new();
            let ca_blur_v = rpd2.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.quarter_tex {
                ca_blur_v.set_texture(Some(dst));
            }
            ca_blur_v.set_load_action(MTLLoadAction::DontCare);
            ca_blur_v.set_store_action(MTLStoreAction::Store);
            let enc2 = cmd.new_render_command_encoder(&rpd2);
            enc2.set_render_pipeline_state(&self.pso_blur);
            if let Some(sam) = &self.sampler {
                enc2.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(tmp) = &self.quarter_tmp_tex {
                enc2.set_fragment_texture(0, Some(tmp));
            }
            enc2.set_scissor_rect(sc_quarter);
            let params_v: [f32; 4] = [0.0, 1.0, sigma / 4.0, 0.0];
            enc2.set_fragment_bytes(
                1,
                core::mem::size_of_val(&params_v) as u64,
                params_v.as_ptr() as *const _,
            );
            enc2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px = self
                .prepass_shaded_px
                .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
            enc2.end_encoding();

            // 5) Upsample quarter -> half (scale 2)
            let rpd_us1 = RenderPassDescriptor::new();
            let ca_us1 = rpd_us1.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.half_tex {
                ca_us1.set_texture(Some(dst));
            }
            ca_us1.set_load_action(MTLLoadAction::DontCare);
            ca_us1.set_store_action(MTLStoreAction::Store);
            let enc_us1 = cmd.new_render_command_encoder(&rpd_us1);
            enc_us1.set_render_pipeline_state(&self.pso_upsample);
            if let Some(sam) = &self.sampler {
                enc_us1.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.quarter_tex {
                enc_us1.set_fragment_texture(0, Some(src));
            }
            let scale2: f32 = 2.0;
            enc_us1.set_fragment_bytes(
                1,
                core::mem::size_of_val(&scale2) as u64,
                &scale2 as *const _ as *const _,
            );
            enc_us1.set_scissor_rect(sc_half);
            enc_us1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px =
                self.prepass_shaded_px.saturating_add(sc_half.width.saturating_mul(sc_half.height));
            enc_us1.end_encoding();

            // 6) Upsample half -> prepass (scale 2)
            let rpd_us2 = RenderPassDescriptor::new();
            let ca_us2 = rpd_us2.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.prepass_tex {
                ca_us2.set_texture(Some(dst));
            }
            ca_us2.set_load_action(MTLLoadAction::DontCare);
            ca_us2.set_store_action(MTLStoreAction::Store);
            let enc_us2 = cmd.new_render_command_encoder(&rpd_us2);
            enc_us2.set_render_pipeline_state(&self.pso_upsample);
            if let Some(sam) = &self.sampler {
                enc_us2.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.half_tex {
                enc_us2.set_fragment_texture(0, Some(src));
            }
            enc_us2.set_fragment_bytes(
                1,
                core::mem::size_of_val(&scale2) as u64,
                &scale2 as *const _ as *const _,
            );
            enc_us2.set_scissor_rect(MTLScissorRect {
                x: sc_x,
                y: sc_y,
                width: sc_w,
                height: sc_h,
            });
            enc_us2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px =
                self.prepass_shaded_px.saturating_add(sc_w.saturating_mul(sc_h));
            enc_us2.end_encoding();
        }

        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        let direct_target = if direct_present {
            Some(unsafe { TextureRef::from_ptr(self.pending_present_texture as *mut MTLTexture) })
        } else {
            None
        };
        if self.sample_count > 1 {
            if let Some(msaa) = &self.target_msaa_tex {
                ca0.set_texture(Some(msaa));
            }
            if let Some(dst) = direct_target {
                ca0.set_resolve_texture(Some(dst));
            } else if let Some(dst) = &self.target_tex {
                ca0.set_resolve_texture(Some(dst));
            }
            ca0.set_store_action(MTLStoreAction::MultisampleResolve);
        } else {
            if let Some(dst) = direct_target {
                ca0.set_texture(Some(dst));
            } else if let Some(dst) = &self.target_tex {
                ca0.set_texture(Some(dst));
            }
            ca0.set_store_action(MTLStoreAction::Store);
        }
        // Heuristic: use Load (damage) only when enabled and coverage < threshold
        let dmg_thresh: f32 = self.damage_use_thresh;
        let use_damage = !direct_present
            && self.sample_count == 1
            && self.damage_enabled
            && self.frame_scissor_dp.is_some()
            && self.frame_damage_pct < dmg_thresh;
        if use_damage {
            ca0.set_load_action(MTLLoadAction::Load);
        } else {
            ca0.set_load_action(MTLLoadAction::Clear);
        }
        ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
        let enc = cmd.new_render_command_encoder(&rpd);
        // Move out per-frame to avoid double-borrow on &mut self
        let mut pf = core::mem::take(&mut self.frames[slot]);
        // Optional pre-filtering by frame scissor to reduce CPU work (small damage only)
        let list_main_storage;
        let list_main_ref: &api::DrawList = if use_damage {
            if let Some(sc) = self.frame_scissor_dp {
                if self.frame_damage_pct <= self.damage_prefilter_thresh {
                    list_main_storage = filter_drawlist_by_dp_scissor(list, sc);
                    if list_main_storage.items.len() < list.items.len() {
                        self.acc_culled = self.acc_culled.saturating_add(
                            (list.items.len() - list_main_storage.items.len()) as u32,
                        );
                    }
                    &list_main_storage
                } else {
                    list
                }
            } else {
                list
            }
        } else {
            list
        };
        encode_draws(
            &enc,
            &mut pf,
            self,
            list_main_ref,
            false,
            if use_damage { self.frame_scissor_dp } else { None },
        );
        self.frames[slot] = pf;
        enc.end_encoding();

        // Snapshot last stats
        self.last_stats.vb_bytes = self.frames[slot].vb_used as u64;
        self.last_stats.ub_bytes = self.frames[slot].ub_used as u64;
        self.last_stats.ib_bytes = self.frames[slot].ib_used as u64;
        self.last_stats.draws = self.acc_draws;
        self.last_stats.instanced = self.acc_instanced;
        self.last_stats.icb_cmds = self.acc_icb_cmds;
        self.last_stats.encode_ms = cpu_t0.elapsed().as_secs_f64() * 1000.0;
        self.last_stats.damage_px = self.frame_damage_px;
        self.last_stats.damage_pct = self.frame_damage_pct;
        self.last_stats.damage_rects = self.frame_damage_rects;
        self.last_stats.culled = self.acc_culled;
        // Adaptive stats
        self.last_stats.blur_ms = blur_ms_out;
        self.last_stats.blur_updates = blur_updated;
        self.last_stats.blur_period_ms =
            (self.cam_update_period.as_millis() as u64).min(u64::from(u32::MAX)) as u32;
        self.last_stats.cam_coverage_pct = cam_coverage;
        self.last_stats.cam_paused = if self.cam_paused { 1 } else { 0 };
        self.last_stats.thermal = therm as u8;
        self.last_stats.low_power = if lpm { 1 } else { 0 };
        self.last_stats.cam_width = self.last_cam_w.max(0) as u32;
        self.last_stats.cam_height = self.last_cam_h.max(0) as u32;
        self.last_stats.cam_bit_depth = self.last_cam_bd.max(0) as u8;
        self.last_stats.cam_matrix = self.last_cam_mx.max(0) as u8;
        self.last_stats.cam_video_range = self.last_cam_vr.max(0) as u8;
        self.last_stats.cam_color_space = self.last_cam_cs.max(0) as u8;
    }

    fn submit(&mut self, _token: api::FrameToken) -> Result<(), api::RenderError> {
        if self.submit_error_flag.swap(false, Ordering::AcqRel) {
            return Err(api::RenderError::DeviceLost);
        }
        let slot = (self.frame_id % FRAME_RING_SIZE as u64) as usize;
        let pending_present_drawable = self.pending_present_drawable as *mut core::ffi::c_void;
        self.pending_present_drawable = 0;
        self.pending_present_texture = 0;
        if let Some(cmd) = self.frames[slot].cmd.take() {
            let frame_id = self.frame_id;
            let log_enabled = ios_log_enabled();
            let submit_error_flag = self.submit_error_flag.clone();
            let in_flight = self.frames[slot].in_flight.clone();
            if !pending_present_drawable.is_null() {
                let raw_drawable = pending_present_drawable as *mut MTLDrawable;
                let drawable = unsafe { DrawableRef::from_ptr(raw_drawable) };
                cmd.present_drawable(drawable);
            }
            let completion = ConcreteBlock::new(move |buffer: &CommandBufferRef| {
                let status = buffer.status();
                if log_enabled {
                    ios_log(&format!(
                        "oxide.renderer-metal: submit completion frame={} status={:?}",
                        frame_id, status
                    ));
                }
                if status == MTLCommandBufferStatus::Error {
                    if log_enabled {
                        unsafe {
                            let err: *mut Object = msg_send![buffer, error];
                            if !err.is_null() {
                                let code: i64 = msg_send![err, code];
                                let domain_obj: *mut Object = msg_send![err, domain];
                                let desc_obj: *mut Object = msg_send![err, localizedDescription];
                                let domain = nsstring_to_string(domain_obj)
                                    .unwrap_or_else(|| "<null-domain>".to_string());
                                let desc = nsstring_to_string(desc_obj)
                                    .unwrap_or_else(|| "<null-description>".to_string());
                                ios_log(&format!(
                                    "oxide.renderer-metal: submit error frame={} domain={} code={} desc={}",
                                    frame_id, domain, code, desc
                                ));
                            } else {
                                ios_log(&format!(
                                    "oxide.renderer-metal: submit error frame={} error=nil",
                                    frame_id
                                ));
                            }
                        }
                    }
                    submit_error_flag.store(true, Ordering::Release);
                }
                in_flight.store(false, Ordering::Release);
            })
            .copy();
            cmd.add_completed_handler(&completion);
            self.frames[slot].mark_submitted(&cmd);
            cmd.commit();
        }
        Ok(())
    }

    fn resize(&mut self, w: u32, h: u32, scale: f32) -> Result<(), api::RenderError> {
        let next_w = w.max(1);
        let next_h = h.max(1);
        let next_scale = if scale > 0.0 { scale } else { 1.0 };
        if self.target_w == next_w
            && self.target_h == next_h
            && (self.target_scale - next_scale).abs() <= f32::EPSILON
            && self.target_tex.is_some()
        {
            return Ok(());
        }
        self.target_w = next_w;
        self.target_h = next_h;
        self.target_scale = next_scale;
        self.ensure_target();
        Ok(())
    }
}

#[inline(always)]
fn intersect_scissor_dp(a: api::RectI, b: api::RectI) -> api::RectI {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    if x2 > x1 && y2 > y1 {
        api::RectI { x: x1, y: y1, w: x2 - x1, h: y2 - y1 }
    } else {
        api::RectI { x: 0, y: 0, w: 0, h: 0 }
    }
}

#[inline(always)]
fn effective_scissor_dp(
    current: Option<api::RectI>,
    global: Option<api::RectI>,
) -> Option<api::RectI> {
    match (current, global) {
        (Some(c), Some(g)) => Some(intersect_scissor_dp(c, g)),
        (Some(c), None) => Some(c),
        (None, Some(g)) => Some(g),
        (None, None) => None,
    }
}

#[inline(always)]
fn apply_scissor_dp(
    enc: &RenderCommandEncoderRef,
    r: &mut MetalRenderer,
    effective: Option<api::RectI>,
    last_applied: &mut Option<api::RectI>,
) {
    if *last_applied == effective {
        return;
    }
    let scale = r.target_scale.max(1.0);
    let (x, y, w, h) = match effective {
        Some(rc) => {
            let x = (rc.x as f32 * scale).floor() as i32;
            let y = (rc.y as f32 * scale).floor() as i32;
            let w = (rc.w as f32 * scale).ceil() as i32;
            let h = (rc.h as f32 * scale).ceil() as i32;
            (x, y, w, h)
        }
        None => (0, 0, r.target_w as i32, r.target_h as i32),
    };
    let tx = 0;
    let ty = 0;
    let tw = r.target_w as i32;
    let th = r.target_h as i32;
    let x1 = x.clamp(tx, tx + tw);
    let y1 = y.clamp(ty, ty + th);
    let x2 = (x + w).clamp(tx, tx + tw);
    let y2 = (y + h).clamp(ty, ty + th);
    let xr = x1.max(0) as u64;
    let yr = y1.max(0) as u64;
    let wr = (x2 - x1).max(0) as u64;
    let hr = (y2 - y1).max(0) as u64;
    enc.set_scissor_rect(MTLScissorRect { x: xr, y: yr, width: wr, height: hr });
    *last_applied = effective;
    r.scissor_changes = r.scissor_changes.saturating_add(1);
}

fn encode_draws(
    enc: &RenderCommandEncoderRef,
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    list: &api::DrawList,
    prepass: bool,
    global_scissor_dp: Option<api::RectI>,
) {
    let debug_stride = encode_debug_stride();
    // Scissor state
    let mut stack: alloc::vec::Vec<api::RectI> = alloc::vec::Vec::new();
    let mut current: Option<api::RectI> = None;
    let mut last_applied: Option<api::RectI> = None;

    let vp_dp: [f32; 2] = [
        (r.target_w as f32) / r.target_scale.max(1.0),
        (r.target_h as f32) / r.target_scale.max(1.0),
    ];

    let mut i: usize = 0;
    while i < list.items.len() {
        if debug_stride > 0 && (i == 0 || (i % debug_stride) == 0) {
            ios_log(&format!(
                "oxide.renderer-metal: encode frame={} prepass={} idx={} total={} kind={}",
                r.frame_id,
                prepass,
                i,
                list.items.len(),
                draw_cmd_kind(&list.items[i])
            ));
        }
        match &list.items[i] {
            api::DrawCmd::CameraBg { rect, tint, alpha, grayscale, blur, sigma } => {
                // Live camera frames are iOS-only. The synthetic benchmark source is also
                // available on macOS so shader correctness can be tested off-device.
                let camera_preview_supported = cfg!(target_os = "ios")
                    || r.camera_texture_source == CameraTextureSource::SyntheticBenchmark;
                if !camera_preview_supported {
                    i += 1;
                    continue;
                }
                if !r.use_camera_textures {
                    let a = (tint.a * *alpha).clamp(0.0, 1.0);
                    let vparams: [f32; 4] = [rect.x, rect.y, rect.w, rect.h];
                    let fparams: [f32; 12] = [
                        rect.x, rect.y, rect.w, rect.h, 0.0, 0.0, 0.0, 0.0, tint.r, tint.g, tint.b,
                        a,
                    ];
                    enc.set_render_pipeline_state(&r.pso_rrect);
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&vparams) as u64,
                        vparams.as_ptr() as *const _,
                    );
                    enc.set_fragment_bytes(
                        1,
                        core::mem::size_of_val(&fparams) as u64,
                        fparams.as_ptr() as *const _,
                    );
                    enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
                    r.acc_draws = r.acc_draws.saturating_add(1);
                    i += 1;
                    continue;
                }
                if *blur {
                    if r.cam_blur_tex.is_some() {
                        let a = (tint.a * *alpha).clamp(0.0, 1.0);
                        let vbuf: [f32; 4] = [rect.x, rect.y, rect.w, rect.h];
                        let base_fb: [f32; 8] =
                            [rect.x, rect.y, rect.w, rect.h, tint.r, tint.g, tint.b, a];
                        let mut fade_prev = 0.0f32;
                        let mut fade_cur = 1.0f32;
                        if let Some(t0) = r.cam_xfade_t0 {
                            let dt = t0.elapsed().as_millis() as u32;
                            let ms = r.cam_xfade_ms.max(1);
                            let f = (dt as f32 / ms as f32).clamp(0.0, 1.0);
                            fade_prev = 1.0 - f;
                            fade_cur = f;
                        } else if let Some(t0) = r.cam_blur_fade_t0 {
                            let dt = t0.elapsed().as_millis() as u32;
                            let ms = r.cam_xfade_ms.max(1);
                            let f = (dt as f32 / ms as f32).clamp(0.0, 1.0);
                            fade_prev = 0.0;
                            fade_cur = f;
                            // Draw the live or synthetic camera base with (1 - f)
                            if fade_cur < 1.0 {
                                if let Some((cw, ch, bd, mx, vr, cs)) = r.encode_camera_quad(
                                    enc,
                                    vp_dp,
                                    [rect.x, rect.y, rect.w, rect.h],
                                    *tint,
                                    a * (1.0 - fade_cur),
                                    *grayscale,
                                ) {
                                    r.last_cam_w = cw;
                                    r.last_cam_h = ch;
                                    r.last_cam_bd = bd;
                                    r.last_cam_mx = mx;
                                    r.last_cam_vr = vr;
                                    r.last_cam_cs = cs;
                                    r.acc_instanced += 1;
                                }
                            }
                        }
                        enc.set_render_pipeline_state(&r.pso_backdrop);
                        if let Some(sam) = &r.sampler {
                            enc.set_fragment_sampler_state(0, Some(sam));
                        }
                        enc.set_vertex_bytes(
                            1,
                            core::mem::size_of_val(&vp_dp) as u64,
                            vp_dp.as_ptr() as *const _,
                        );
                        enc.set_vertex_bytes(
                            0,
                            core::mem::size_of_val(&vbuf) as u64,
                            vbuf.as_ptr() as *const _,
                        );
                        // Draw previous blurred
                        if fade_prev > 0.0 {
                            if let Some(prev) = &r.cam_xfade_prev_tex {
                                enc.set_fragment_texture(0, Some(prev));
                                let mut fb = base_fb;
                                fb[7] = a * fade_prev;
                                enc.set_fragment_bytes(
                                    1,
                                    core::mem::size_of_val(&fb) as u64,
                                    fb.as_ptr() as *const _,
                                );
                                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                                r.acc_instanced += 1;
                            }
                        }
                        // Draw current blurred
                        if let Some(src) = &r.cam_blur_tex {
                            enc.set_fragment_texture(0, Some(src));
                            let mut fb = base_fb;
                            fb[7] = a * fade_cur;
                            enc.set_fragment_bytes(
                                1,
                                core::mem::size_of_val(&fb) as u64,
                                fb.as_ptr() as *const _,
                            );
                            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            r.acc_instanced += 1;
                        }
                    }
                } else {
                    if let Some((cw, ch, bd, mx, vr, cs)) = r.encode_camera_quad(
                        enc,
                        vp_dp,
                        [rect.x, rect.y, rect.w, rect.h],
                        *tint,
                        (tint.a * *alpha).clamp(0.0, 1.0),
                        *grayscale,
                    ) {
                        r.last_cam_w = cw;
                        r.last_cam_h = ch;
                        r.last_cam_bd = bd;
                        r.last_cam_mx = mx;
                        r.last_cam_vr = vr;
                        r.last_cam_cs = cs;
                        r.acc_draws = r.acc_draws.saturating_add(1);
                    }
                }
                i += 1;
                continue;
            }
            api::DrawCmd::LayerBegin { id, rect, dirty } => {
                // Find matching LayerEnd and collect sublist
                let mut depth = 1usize;
                let mut j = i + 1;
                while j < list.items.len() && depth > 0 {
                    match &list.items[j] {
                        api::DrawCmd::LayerBegin { .. } => depth += 1,
                        api::DrawCmd::LayerEnd => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let end = j - 1; // points to LayerEnd
                                 // If in prepass, render sublist inline (no caching)
                if prepass {
                    // Encode sublist directly
                    let sub = api::DrawList {
                        items: list.items[i + 1..end].to_vec(),
                        vertices: list.vertices.clone(),
                        indices: list.indices.clone(),
                    };
                    let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                    encode_draws(enc, pf, r, &sub, true, global_scissor_dp);
                    apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
                    i = end + 1;
                    continue;
                }
                // Determine if sublist contains unsupported commands (Solid)
                let mut unsupported = false;
                for k in i + 1..end {
                    if matches!(list.items[k], api::DrawCmd::Solid { .. }) {
                        unsupported = true;
                        break;
                    }
                }
                if unsupported {
                    // Fallback to inline encode
                    let sub = api::DrawList {
                        items: list.items[i + 1..end].to_vec(),
                        vertices: list.vertices.clone(),
                        indices: list.indices.clone(),
                    };
                    let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                    encode_draws(enc, pf, r, &sub, false, global_scissor_dp);
                    apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
                    i = end + 1;
                    continue;
                }
                if !r.layer_cache_enabled {
                    // Correctness-first path: disable layer texture caching and render inline.
                    let sub = api::DrawList {
                        items: list.items[i + 1..end].to_vec(),
                        vertices: list.vertices.clone(),
                        indices: list.indices.clone(),
                    };
                    let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                    encode_draws(enc, pf, r, &sub, false, global_scissor_dp);
                    apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
                    i = end + 1;
                    continue;
                }
                // Build offset sublist in local coordinates (dp) and compute hash
                let ox = rect.x;
                let oy = rect.y;
                let mut sub = api::DrawList {
                    items: alloc::vec::Vec::new(),
                    vertices: alloc::vec::Vec::new(),
                    indices: alloc::vec::Vec::new(),
                };
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                for k in i + 1..end {
                    match &list.items[k] {
                        api::DrawCmd::ClipPush { rect: r0 } => {
                            let mut rr = *r0;
                            rr.x -= ox as i32;
                            rr.y -= oy as i32;
                            sub.items.push(api::DrawCmd::ClipPush { rect: rr });
                            rr.x.hash(&mut hasher);
                            rr.y.hash(&mut hasher);
                            rr.w.hash(&mut hasher);
                            rr.h.hash(&mut hasher);
                        }
                        api::DrawCmd::CameraBg {
                            rect: r0,
                            tint,
                            alpha,
                            grayscale,
                            blur,
                            sigma,
                        } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::CameraBg {
                                rect: adj,
                                tint: *tint,
                                alpha: *alpha,
                                grayscale: *grayscale,
                                blur: *blur,
                                sigma: *sigma,
                            });
                            ((adj.x.to_bits() ^ adj.y.to_bits()) as u64).hash(&mut hasher);
                        }
                        api::DrawCmd::ClipPop => sub.items.push(api::DrawCmd::ClipPop),
                        api::DrawCmd::RRect { rect: r0, radii, color } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::RRect {
                                rect: adj,
                                radii: *radii,
                                color: *color,
                            });
                            ((adj.x.to_bits() ^ adj.y.to_bits()) as u64).hash(&mut hasher);
                        }
                        api::DrawCmd::NineSlice { tex, rect: r0, slice, alpha } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::NineSlice {
                                tex: *tex,
                                rect: adj,
                                slice: *slice,
                                alpha: *alpha,
                            });
                            tex.0.hash(&mut hasher);
                        }
                        api::DrawCmd::Image { tex, dst, src, alpha } => {
                            let adj = api::RectF::new(dst.x - ox, dst.y - oy, dst.w, dst.h);
                            sub.items.push(api::DrawCmd::Image {
                                tex: *tex,
                                dst: adj,
                                src: *src,
                                alpha: *alpha,
                            });
                            tex.0.hash(&mut hasher);
                        }
                        api::DrawCmd::Spinner { center, atom, alpha } => {
                            let adj = [center[0] - ox, center[1] - oy];
                            sub.items.push(api::DrawCmd::Spinner {
                                center: adj,
                                atom: *atom,
                                alpha: *alpha,
                            });
                        }
                        api::DrawCmd::Backdrop { rect: r0, sigma, tint, alpha } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::Backdrop {
                                rect: adj,
                                sigma: *sigma,
                                tint: *tint,
                                alpha: *alpha,
                            });
                        }
                        api::DrawCmd::GlyphRun { run } => {
                            // Copy referenced vertices/indices with rebase
                            let v_count = run.vb.len as usize;
                            let i_count = run.ib.len as usize;
                            let new_v_off = sub.vertices.len() as u32;
                            // Copy and offset verts
                            let srcv = &list.vertices
                                [(run.vb.offset as usize)..(run.vb.offset as usize + v_count)];
                            for v in srcv.iter() {
                                let mut vv = *v;
                                vv.x -= ox;
                                vv.y -= oy;
                                sub.vertices.push(vv);
                            }
                            // Copy and rebase indices
                            let srci = &list.indices
                                [(run.ib.offset as usize)..(run.ib.offset as usize + i_count)];
                            let Some(remapped_indices) =
                                remap_indices_to_span(srci, run.vb.offset, run.vb.len, new_v_off)
                            else {
                                continue;
                            };
                            sub.indices.extend_from_slice(&remapped_indices);
                            sub.items.push(api::DrawCmd::GlyphRun {
                                run: api::GlyphRun {
                                    atlas: run.atlas,
                                    vb: api::VertexSpan { offset: new_v_off, len: v_count as u32 },
                                    ib: api::IndexSpan {
                                        offset: (sub.indices.len() as u32)
                                            .wrapping_sub(i_count as u32),
                                        len: i_count as u32,
                                    },
                                    sdf: run.sdf,
                                    color: run.color,
                                },
                            });
                        }
                        _ => {}
                    }
                }
                let hash = hasher.finish();
                // Ensure layer texture exists (px)
                let w_px = (rect.w * r.target_scale.max(1.0)).ceil() as u32;
                let h_px = (rect.h * r.target_scale.max(1.0)).ceil() as u32;
                let do_render = *dirty
                    || !r.layers.get(id).is_some()
                    || r.layers
                        .get(id)
                        .map(|e| e.w != w_px || e.h != h_px || e.hash != hash)
                        .unwrap_or(true);
                if do_render {
                    // If the cache did not get refreshed in pre-scan, render inline.
                    // This avoids composing stale or empty layer textures.
                    let sub = api::DrawList {
                        items: list.items[i + 1..end].to_vec(),
                        vertices: list.vertices.clone(),
                        indices: list.indices.clone(),
                    };
                    let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                    encode_draws(enc, pf, r, &sub, false, global_scissor_dp);
                    apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
                    i = end + 1;
                    continue;
                }
                // Composite the cached layer via nine-slice (no slicing)
                if let Some(layer) = r.layers.get(id) {
                    enc.set_render_pipeline_state(&r.pso_nine_slice);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(&layer.tex));
                    // Vertex params: rect dp + vp dp
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    let vparams: [f32; 6] = [rect.x, rect.y, rect.w, rect.h, vp_dp[0], vp_dp[1]];
                    enc.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&vparams) as u64,
                        vparams.as_ptr() as *const _,
                    );
                    // Fragment params are in dp space for rect, with texture size in px.
                    let params = pack_nine_slice_params(
                        *rect,
                        layer.w as f32,
                        layer.h as f32,
                        api::Insets::new(0.0, 0.0, 0.0, 0.0),
                        1.0,
                    );
                    enc.set_fragment_bytes(
                        1,
                        core::mem::size_of_val(&params) as u64,
                        (&params as *const NineSliceGpuParams).cast(),
                    );
                    enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
                    r.acc_draws += 1;
                }
                i = end + 1;
                continue;
            }
            api::DrawCmd::LayerEnd => {
                i += 1;
                continue;
            }
            api::DrawCmd::ClipPush { rect } => {
                let next =
                    if let Some(cur) = current { intersect_scissor_dp(cur, *rect) } else { *rect };
                stack.push(*rect);
                current = Some(next);
                let effective = effective_scissor_dp(current, global_scissor_dp);
                apply_scissor_dp(enc, r, effective, &mut last_applied);
                i += 1;
                continue;
            }
            api::DrawCmd::ClipPop => {
                let _ = stack.pop();
                current = if stack.is_empty() {
                    None
                } else {
                    let mut it = stack.iter();
                    let mut acc = *it.next().unwrap();
                    for rct in it {
                        acc = intersect_scissor_dp(acc, *rct);
                        if acc.w == 0 || acc.h == 0 {
                            break;
                        }
                    }
                    Some(acc)
                };
                let effective = effective_scissor_dp(current, global_scissor_dp);
                apply_scissor_dp(enc, r, effective, &mut last_applied);
                i += 1;
                continue;
            }
            api::DrawCmd::Solid { vb, ib, color } => {
                enc.set_render_pipeline_state(&r.pso_solid);
                let v_count = vb.len as usize;
                let v_bytes = v_count * core::mem::size_of::<api::Vertex>();
                let slot = (r.frame_id % FRAME_RING_SIZE as u64) as usize;
                r.vb.ensure_capacity(&r.device, slot, pf.vb_used + v_bytes);
                let dst = unsafe {
                    core::slice::from_raw_parts_mut(
                        r.vb.contents_ptr(slot).as_ptr().add(pf.vb_used),
                        v_bytes,
                    )
                };
                let src_slice =
                    &list.vertices[(vb.offset as usize)..(vb.offset as usize + v_count)];
                let mut clip_vertices = alloc::vec::Vec::with_capacity(v_count);
                for vertex in src_slice.iter().copied() {
                    clip_vertices.push(map_solid_vertex_dp_to_clip(vertex, vp_dp[0], vp_dp[1]));
                }
                let src_bytes: &[u8] = unsafe {
                    core::slice::from_raw_parts(clip_vertices.as_ptr() as *const u8, v_bytes)
                };
                dst.copy_from_slice(src_bytes);
                let vb_off = pf.vb_used as u64;
                pf.vb_used += v_bytes;
                let rgba = [color.r, color.g, color.b, color.a];
                let ub_off = pf.ub_used as u64;
                let u_bytes = core::mem::size_of_val(&rgba);
                r.ub.ensure_capacity(&r.device, slot, pf.ub_used + u_bytes);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        rgba.as_ptr() as *const u8,
                        r.ub.contents_ptr(slot).as_ptr().add(pf.ub_used),
                        u_bytes,
                    );
                }
                pf.ub_used += u_bytes;
                enc.set_vertex_buffer(0, Some(&r.vb.bufs[slot]), vb_off);
                enc.set_fragment_buffer(0, Some(&r.ub.bufs[slot]), ub_off);
                let idx_count = ib.len as usize;
                if idx_count > 0 {
                    // Upload indices and draw indexed
                    let isrc_slice =
                        &list.indices[(ib.offset as usize)..(ib.offset as usize + idx_count)];
                    let Some(local_indices) =
                        normalize_indices_for_local_vertex_span(isrc_slice, vb.offset, vb.len)
                    else {
                        i += 1;
                        continue;
                    };
                    let i_bytes = local_indices.len() * core::mem::size_of::<u16>();
                    r.ib.ensure_capacity(&r.device, slot, pf.ib_used + i_bytes);
                    let idst = unsafe {
                        core::slice::from_raw_parts_mut(
                            r.ib.contents_ptr(slot).as_ptr().add(pf.ib_used),
                            i_bytes,
                        )
                    };
                    let isrc_bytes: &[u8] = unsafe {
                        core::slice::from_raw_parts(local_indices.as_ptr() as *const u8, i_bytes)
                    };
                    idst.copy_from_slice(isrc_bytes);
                    let ib_off = pf.ib_used as u64;
                    pf.ib_used += i_bytes;
                    if let Some(primitive) = solid_primitive_for_index_count(local_indices.len()) {
                        enc.draw_indexed_primitives(
                            primitive,
                            local_indices.len() as u64,
                            MTLIndexType::UInt16,
                            &r.ib.bufs[slot],
                            ib_off,
                        );
                        r.acc_draws += 1;
                    }
                } else {
                    if let Some(primitive) = solid_primitive_for_vertex_count(v_count) {
                        enc.draw_primitives(primitive, 0, v_count as u64);
                        r.acc_draws += 1;
                    }
                }
                i += 1;
            }
            api::DrawCmd::RRect { rect, radii, color } => {
                enc.set_render_pipeline_state(&r.pso_rrect);
                // Draw consecutive RRects in a deterministic non-instanced loop.
                // Some simulator/driver combinations can produce flat output for
                // instanced UI quads; this path prioritizes correctness.
                let mut j = i;
                while j < list.items.len() && matches!(list.items[j], api::DrawCmd::RRect { .. }) {
                    if let api::DrawCmd::RRect { rect, radii, color } = &list.items[j] {
                        let vparams: [f32; 4] = [rect.x, rect.y, rect.w, rect.h];
                        let fparams: [f32; 12] = [
                            rect.x, rect.y, rect.w, rect.h, radii[0], radii[1], radii[2], radii[3],
                            color.r, color.g, color.b, color.a,
                        ];
                        enc.set_vertex_bytes(
                            1,
                            core::mem::size_of_val(&vp_dp) as u64,
                            vp_dp.as_ptr() as *const _,
                        );
                        enc.set_vertex_bytes(
                            0,
                            core::mem::size_of_val(&vparams) as u64,
                            vparams.as_ptr() as *const _,
                        );
                        enc.set_fragment_bytes(
                            1,
                            core::mem::size_of_val(&fparams) as u64,
                            fparams.as_ptr() as *const _,
                        );
                        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
                        r.acc_draws = r.acc_draws.saturating_add(1);
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                if let Some(img) = r.get_image_tex(*tex) {
                    enc.set_render_pipeline_state(&r.pso_nine_slice);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(img));
                    // Batch consecutive NineSlice with same texture
                    let mut count = 0usize;
                    let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                    let mut fbuf: alloc::vec::Vec<NineSliceGpuParams> = alloc::vec::Vec::new();
                    let tex_w = img.width() as f32;
                    let tex_h = img.height() as f32;
                    let mut j = i;
                    while j < list.items.len() {
                        if let api::DrawCmd::NineSlice { tex: t2, rect, slice, alpha } =
                            &list.items[j]
                        {
                            if *t2 != *tex {
                                break;
                            }
                            vbuf.extend_from_slice(&[rect.x, rect.y, rect.w, rect.h]);
                            fbuf.push(pack_nine_slice_params(*rect, tex_w, tex_h, *slice, *alpha));
                            count += 1;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    let max_batch = max_instances_per_set_bytes(
                        core::mem::size_of::<[f32; 4]>(),
                        core::mem::size_of::<NineSliceGpuParams>(),
                    );
                    let mut emitted = 0usize;
                    let mut start = 0usize;
                    while start < count {
                        let end = (start + max_batch).min(count);
                        let v_slice = &vbuf[(start * 4)..(end * 4)];
                        let f_slice = &fbuf[start..end];
                        enc.set_vertex_bytes(
                            0,
                            (v_slice.len() * core::mem::size_of::<f32>()) as u64,
                            v_slice.as_ptr() as *const _,
                        );
                        enc.set_fragment_bytes(
                            1,
                            (f_slice.len() * core::mem::size_of::<NineSliceGpuParams>()) as u64,
                            f_slice.as_ptr() as *const _,
                        );
                        enc.draw_primitives_instanced(
                            MTLPrimitiveType::Triangle,
                            0,
                            6,
                            (end - start) as u64,
                        );
                        emitted += end - start;
                        start = end;
                    }
                    r.acc_instanced += emitted as u32;
                    i = j;
                    continue;
                }
                i += 1;
            }
            api::DrawCmd::Image { .. } => {
                if let Some(sam) = &r.sampler {
                    enc.set_fragment_sampler_state(0, Some(sam));
                }
                // Simulator-safe image path: avoid argument-buffer texturing, which has
                // repeatedly produced MTLSim command-buffer faults under heavy scene loads.
                if !r.use_image_arg_buffer {
                    enc.set_render_pipeline_state(&r.pso_image_single);
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    let mut emitted = 0usize;
                    let mut j = i;
                    while j < list.items.len() {
                        if let api::DrawCmd::Image { tex, dst, src, alpha } = &list.items[j] {
                            if let Some(tref) = r.get_image_tex(*tex) {
                                let vparams: [f32; 4] = [dst.x, dst.y, dst.w, dst.h];
                                let (tw, th) = (tref.width() as f32, tref.height() as f32);
                                let fparams = pack_image_params(
                                    *dst,
                                    *src,
                                    [tw, th],
                                    (*alpha).clamp(0.0, 1.0),
                                    0,
                                );
                                enc.set_fragment_texture(0, Some(tref));
                                enc.set_vertex_bytes(
                                    0,
                                    core::mem::size_of_val(&vparams) as u64,
                                    vparams.as_ptr() as *const _,
                                );
                                enc.set_fragment_bytes(
                                    1,
                                    core::mem::size_of_val(&fparams) as u64,
                                    (&fparams as *const ImageGpuParams).cast(),
                                );
                                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                                emitted += 1;
                            }
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    r.acc_draws = r.acc_draws.saturating_add(emitted as u32);
                    i = j;
                    continue;
                }

                enc.set_render_pipeline_state(&r.pso_image);
                // Bind argument buffer for image textures
                if let Some(buf) = &r.img_arg_buf {
                    enc.set_fragment_buffer(2, Some(buf), 0);
                }
                // Batch consecutive Images regardless of texture using argument buffer
                let mut count = 0usize;
                let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                let mut fbuf: alloc::vec::Vec<ImageGpuParams> = alloc::vec::Vec::new();
                let mut tex_map: std::collections::HashMap<u32, u32> =
                    std::collections::HashMap::new();
                let mut next_slot: u32 = 0;
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::Image { tex, dst, src, alpha } = &list.items[j] {
                        let Some(tref) = r.get_image_tex(*tex) else {
                            // Skip image draws referencing unknown textures to avoid sampling
                            // unbound argument-buffer slots on simulator/device GPUs.
                            j += 1;
                            continue;
                        };
                        // Map texture handle to slot
                        let slot_idx = if let Some(s) = tex_map.get(&tex.0) {
                            *s
                        } else {
                            if next_slot >= IMAGE_ARG_TEXTURE_SLOTS {
                                break;
                            }
                            let s = next_slot;
                            next_slot += 1;
                            // Set texture in argument encoder
                            if let (Some(encdr), Some(buf)) =
                                (r.img_arg.as_ref(), r.img_arg_buf.as_ref())
                            {
                                // Rebind the buffer to ensure encoder targets it
                                encdr.set_argument_buffer(buf, 0);
                                encdr.set_texture(s as u64, tref);
                            }
                            tex_map.insert(tex.0, s);
                            s
                        };
                        // Vertex params
                        vbuf.extend_from_slice(&[dst.x, dst.y, dst.w, dst.h]);
                        // Fragment params (ImageParams): rect(dp), src(px), tex_size(px), alpha, tex_index
                        let (tw, th) = (tref.width() as f32, tref.height() as f32);
                        fbuf.push(pack_image_params(
                            *dst,
                            *src,
                            [tw, th],
                            (*alpha).clamp(0.0, 1.0),
                            slot_idx,
                        ));
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                if count == 0 {
                    i = j;
                    continue;
                }
                // Set vp
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                let max_batch = max_instances_per_set_bytes(
                    core::mem::size_of::<[f32; 4]>(),
                    core::mem::size_of::<ImageGpuParams>(),
                );
                let mut emitted = 0usize;
                let mut start = 0usize;
                while start < count {
                    let end = (start + max_batch).min(count);
                    let v_slice = &vbuf[(start * 4)..(end * 4)];
                    let f_slice = &fbuf[start..end];
                    enc.set_vertex_bytes(
                        0,
                        (v_slice.len() * core::mem::size_of::<f32>()) as u64,
                        v_slice.as_ptr() as *const _,
                    );
                    enc.set_fragment_bytes(
                        1,
                        (f_slice.len() * core::mem::size_of::<ImageGpuParams>()) as u64,
                        f_slice.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(
                        MTLPrimitiveType::Triangle,
                        0,
                        6,
                        (end - start) as u64,
                    );
                    emitted += end - start;
                    start = end;
                }
                r.acc_instanced += emitted as u32;
                i = j;
                continue;
            }
            api::DrawCmd::GlyphRun { .. } => {
                // Group consecutive GlyphRun with same atlas and sdf flag, and record into ICB
                let mut count = 0usize;
                let mut group_atlas = None;
                let mut group_sdf = false;
                let slot = (r.frame_id % FRAME_RING_SIZE as u64) as usize;
                // Pre-scan to determine group and upload VB/UB/IB, collecting offsets
                struct GR {
                    vb_off: u64,
                    ib_off: u64,
                    idx_count: u64,
                    ub_off: u64,
                }
                let mut group: alloc::vec::Vec<GR> = alloc::vec::Vec::new();
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::GlyphRun { run } = &list.items[j] {
                        if group_atlas.is_none() {
                            group_atlas = Some(run.atlas);
                            group_sdf = run.sdf;
                        } else if group_atlas != Some(run.atlas) || group_sdf != run.sdf {
                            break;
                        }

                        // Upload VB
                        let v_count = run.vb.len as usize;
                        let v_bytes = v_count * core::mem::size_of::<api::Vertex>();
                        r.vb.ensure_capacity(&r.device, slot, pf.vb_used + v_bytes);
                        let dst = unsafe {
                            core::slice::from_raw_parts_mut(
                                r.vb.contents_ptr(slot).as_ptr().add(pf.vb_used),
                                v_bytes,
                            )
                        };
                        let src_slice = &list.vertices
                            [(run.vb.offset as usize)..(run.vb.offset as usize + v_count)];
                        let src_bytes: &[u8] = unsafe {
                            core::slice::from_raw_parts(src_slice.as_ptr() as *const u8, v_bytes)
                        };
                        dst.copy_from_slice(src_bytes);
                        let vb_off = pf.vb_used as u64;
                        pf.vb_used += v_bytes;
                        // Upload color UB
                        let rgba = [run.color.r, run.color.g, run.color.b, run.color.a];
                        let ub_off = pf.ub_used as u64;
                        let u_bytes = core::mem::size_of_val(&rgba);
                        r.ub.ensure_capacity(&r.device, slot, pf.ub_used + u_bytes);
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                rgba.as_ptr() as *const u8,
                                r.ub.contents_ptr(slot).as_ptr().add(pf.ub_used),
                                u_bytes,
                            );
                        }
                        pf.ub_used += u_bytes;
                        // Upload IB
                        let idx_count = run.ib.len as usize;
                        let mut ib_off = 0u64;
                        let mut local_idx_count = 0u64;
                        if idx_count > 0 {
                            let isrc_slice = &list.indices
                                [(run.ib.offset as usize)..(run.ib.offset as usize + idx_count)];
                            if let Some(local_indices) = normalize_indices_for_local_vertex_span(
                                isrc_slice,
                                run.vb.offset,
                                run.vb.len,
                            ) {
                                let i_bytes = local_indices.len() * core::mem::size_of::<u16>();
                                r.ib.ensure_capacity(&r.device, slot, pf.ib_used + i_bytes);
                                let idst = unsafe {
                                    core::slice::from_raw_parts_mut(
                                        r.ib.contents_ptr(slot).as_ptr().add(pf.ib_used),
                                        i_bytes,
                                    )
                                };
                                let isrc_bytes: &[u8] = unsafe {
                                    core::slice::from_raw_parts(
                                        local_indices.as_ptr() as *const u8,
                                        i_bytes,
                                    )
                                };
                                idst.copy_from_slice(isrc_bytes);
                                ib_off = pf.ib_used as u64;
                                pf.ib_used += i_bytes;
                                local_idx_count = local_indices.len() as u64;
                            }
                        }
                        group.push(GR { vb_off, ib_off, idx_count: local_idx_count, ub_off });
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                // Bind atlas + sampler and vp
                if let Some(atlas) = group_atlas.and_then(|h| r.get_image_tex(h)) {
                    if group_sdf {
                        enc.set_render_pipeline_state(&r.pso_text_sdf);
                    } else {
                        enc.set_render_pipeline_state(&r.pso_text);
                    }
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(atlas));
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );

                    if r.use_glyph_icb {
                        // Create ICB and record commands
                        let desc = IndirectCommandBufferDescriptor::new();
                        desc.set_command_types(MTLIndirectCommandType::DrawIndexed);
                        desc.set_inherit_pipeline_state(false);
                        desc.set_max_vertex_buffer_bind_count(1);
                        desc.set_max_fragment_buffer_bind_count(2);
                        let icb = r.device.new_indirect_command_buffer_with_descriptor(
                            &desc,
                            count as u64,
                            glyph_icb_resource_options(),
                        );
                        for (ci, gr) in group.iter().enumerate() {
                            let cmd_i = icb.indirect_render_command_at_index(ci as u64);
                            if group_sdf {
                                cmd_i.set_render_pipeline_state(&r.pso_text_sdf);
                            } else {
                                cmd_i.set_render_pipeline_state(&r.pso_text);
                            }
                            cmd_i.set_vertex_buffer(0, Some(&r.vb.bufs[slot]), gr.vb_off);
                            cmd_i.set_fragment_buffer(0, Some(&r.ub.bufs[slot]), gr.ub_off);
                            if gr.idx_count > 0 {
                                cmd_i.draw_indexed_primitives(
                                    MTLPrimitiveType::Triangle,
                                    gr.idx_count,
                                    MTLIndexType::UInt16,
                                    &r.ib.bufs[slot],
                                    gr.ib_off,
                                    1,
                                    0,
                                    0,
                                );
                            }
                        }
                        enc.execute_commands_in_buffer(
                            &icb,
                            NSRange { location: 0, length: count as u64 },
                        );
                        r.acc_icb_cmds += count as u32;
                    } else {
                        for gr in &group {
                            enc.set_vertex_buffer(0, Some(&r.vb.bufs[slot]), gr.vb_off);
                            enc.set_fragment_buffer(0, Some(&r.ub.bufs[slot]), gr.ub_off);
                            if gr.idx_count > 0 {
                                enc.draw_indexed_primitives(
                                    MTLPrimitiveType::Triangle,
                                    gr.idx_count,
                                    MTLIndexType::UInt16,
                                    &r.ib.bufs[slot],
                                    gr.ib_off,
                                );
                                r.acc_draws = r.acc_draws.saturating_add(1);
                            }
                        }
                    }
                }
                i = j;
                continue;
            }
            api::DrawCmd::Spinner { center, atom, alpha } => {
                enc.set_render_pipeline_state(&r.pso_spinner);
                let phase = legacy_spinner_phase(spinner_now_ms());
                // Batch consecutive spinners
                let mut count = 0usize;
                let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                let mut fbuf: alloc::vec::Vec<SpinnerGpuParams> = alloc::vec::Vec::new();
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::Spinner { center, atom, alpha } = &list.items[j] {
                        let thickness = legacy_spinner_thickness(*atom);
                        let radius = legacy_spinner_radius(*atom);
                        let mm = *atom * 0.5;
                        vbuf.extend_from_slice(&[
                            center[0] - mm,
                            center[1] - mm,
                            mm * 2.0,
                            mm * 2.0,
                        ]);
                        fbuf.push(pack_spinner_params(*center, radius, thickness, phase, *alpha));
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                let max_batch = max_instances_per_set_bytes(
                    core::mem::size_of::<[f32; 4]>(),
                    core::mem::size_of::<SpinnerGpuParams>(),
                );
                let mut emitted = 0usize;
                let mut start = 0usize;
                while start < count {
                    let end = (start + max_batch).min(count);
                    let v_slice = &vbuf[(start * 4)..(end * 4)];
                    let f_slice = &fbuf[start..end];
                    enc.set_vertex_bytes(
                        0,
                        (v_slice.len() * core::mem::size_of::<f32>()) as u64,
                        v_slice.as_ptr() as *const _,
                    );
                    enc.set_fragment_bytes(
                        1,
                        (f_slice.len() * core::mem::size_of::<SpinnerGpuParams>()) as u64,
                        f_slice.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(
                        MTLPrimitiveType::Triangle,
                        0,
                        6,
                        (end - start) as u64,
                    );
                    emitted += end - start;
                    start = end;
                }
                r.acc_instanced += emitted as u32;
                i = j;
                continue;
            }
            api::DrawCmd::Backdrop { rect, tint, alpha, .. } => {
                if prepass {
                    // Stop prepass at the first backdrop; draw nothing for it here.
                    break;
                }
                if let Some(src) = &r.prepass_tex {
                    enc.set_render_pipeline_state(&r.pso_backdrop);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(src));
                    // Batch consecutive backdrops
                    let mut count = 0usize;
                    let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                    let mut fbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new(); // rect px + tint
                    let mut j = i;
                    while j < list.items.len() {
                        if let api::DrawCmd::Backdrop { rect, tint, alpha, .. } = &list.items[j] {
                            vbuf.extend_from_slice(&[rect.x, rect.y, rect.w, rect.h]);
                            let a = (tint.a * *alpha).clamp(0.0, 1.0);
                            fbuf.extend_from_slice(&[
                                rect.x, rect.y, rect.w, rect.h, tint.r, tint.g, tint.b, a,
                            ]);
                            count += 1;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    let max_batch = max_instances_per_set_bytes(
                        core::mem::size_of::<[f32; 4]>(),
                        core::mem::size_of::<[f32; 8]>(),
                    );
                    let mut emitted = 0usize;
                    let mut start = 0usize;
                    while start < count {
                        let end = (start + max_batch).min(count);
                        let v_slice = &vbuf[(start * 4)..(end * 4)];
                        let f_slice = &fbuf[(start * 8)..(end * 8)];
                        enc.set_vertex_bytes(
                            0,
                            (v_slice.len() * core::mem::size_of::<f32>()) as u64,
                            v_slice.as_ptr() as *const _,
                        );
                        enc.set_fragment_bytes(
                            1,
                            (f_slice.len() * core::mem::size_of::<f32>()) as u64,
                            f_slice.as_ptr() as *const _,
                        );
                        enc.draw_primitives_instanced(
                            MTLPrimitiveType::Triangle,
                            0,
                            6,
                            (end - start) as u64,
                        );
                        emitted += end - start;
                        start = end;
                    }
                    r.acc_instanced += emitted as u32;
                    i = j;
                    continue;
                }
                i += 1;
            } // ClipPush/ClipPop handled above
        }
        // Default progress
        // Note: continue branches have updated i accordingly
        if i < list.items.len() { /* fallthrough increment happens in each arm */ }
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
struct NineSliceGpuParams {
    rect: [f32; 4],
    tex_size: [f32; 2],
    _pad0: [f32; 2],
    slice_ltrb: [f32; 4],
    alpha: f32,
    _pad1: [f32; 3],
}

#[inline]
fn pack_nine_slice_params(
    rect: api::RectF,
    tex_w: f32,
    tex_h: f32,
    slice: api::Insets,
    alpha: f32,
) -> NineSliceGpuParams {
    NineSliceGpuParams {
        rect: [rect.x, rect.y, rect.w, rect.h],
        tex_size: [tex_w, tex_h],
        _pad0: [0.0, 0.0],
        slice_ltrb: [slice.left, slice.top, slice.right, slice.bottom],
        alpha: alpha.clamp(0.0, 1.0),
        _pad1: [0.0, 0.0, 0.0],
    }
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug)]
struct SpinnerGpuParams {
    center: [f32; 2],
    radius: f32,
    thickness: f32,
    phase: f32,
    alpha: f32,
}

#[inline]
fn pack_spinner_params(
    center: [f32; 2],
    radius: f32,
    thickness: f32,
    phase: f32,
    alpha: f32,
) -> SpinnerGpuParams {
    SpinnerGpuParams { center, radius, thickness, phase, alpha: alpha.clamp(0.0, 1.0) }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
struct ImageGpuParams {
    rect: [f32; 4],
    src_rect: [f32; 4],
    tex_size: [f32; 2],
    alpha: f32,
    tex_index: u32,
}

#[inline]
fn pack_image_params(
    dst: api::RectF,
    src: api::RectF,
    tex_size: [f32; 2],
    alpha: f32,
    tex_index: u32,
) -> ImageGpuParams {
    ImageGpuParams {
        rect: [dst.x, dst.y, dst.w, dst.h],
        src_rect: [src.x, src.y, src.w, src.h],
        tex_size,
        alpha: alpha.clamp(0.0, 1.0),
        tex_index,
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
struct CameraGpuParams {
    rect: [f32; 4],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_bias: [f32; 2],
    grayscale: f32,
    matrix: f32,
    video_range: f32,
    bit_depth: f32,
    pad: [f32; 4],
}

#[inline]
fn pack_camera_params(
    rect_dp: [f32; 4],
    tint: api::Color,
    alpha: f32,
    uv_scale: [f32; 2],
    uv_bias: [f32; 2],
    grayscale: bool,
    matrix: i32,
    video_range: i32,
    bit_depth: i32,
) -> CameraGpuParams {
    CameraGpuParams {
        rect: rect_dp,
        tint: [tint.r, tint.g, tint.b, alpha.clamp(0.0, 1.0)],
        uv_scale,
        uv_bias,
        grayscale: if grayscale { 1.0 } else { 0.0 },
        matrix: matrix as f32,
        video_range: video_range as f32,
        bit_depth: bit_depth as f32,
        pad: [0.0, 0.0, 0.0, 0.0],
    }
}

#[inline]
fn yuv_to_rgb_bt709_full_range(y: f32, u: f32, v: f32) -> [f32; 3] {
    [
        (y + 1.5748 * v).clamp(0.0, 1.0),
        (y - 0.1873 * u - 0.4681 * v).clamp(0.0, 1.0),
        (y + 1.8556 * u).clamp(0.0, 1.0),
    ]
}

#[inline]
fn linear_to_srgb_u8(value: f32) -> u8 {
    let linear = value.clamp(0.0, 1.0);
    let srgb =
        if linear <= 0.003_130_8 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Clone, Debug)]
struct CameraNv12Source {
    y_tex: Texture,
    uv_tex: Texture,
    width: i32,
    height: i32,
    bit_depth: i32,
    matrix: i32,
    video_range: i32,
    color_space: i32,
}

#[derive(Clone, Debug)]
struct CameraBgraSource {
    tex: Texture,
    width: i32,
    height: i32,
}

#[derive(Clone, Debug)]
struct LiveCameraNv12Frame {
    y_tex: usize,
    uv_tex: usize,
    width: i32,
    height: i32,
    bit_depth: i32,
    matrix: i32,
    video_range: i32,
    color_space: i32,
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    slot: u32,
    generation: u64,
    timestamp_ns: u64,
}

pub const CAMERA_PREVIEW_REASON_SUBMIT_ERROR: u32 = 1 << 0;
pub const CAMERA_PREVIEW_REASON_NON_DIRECT_PREVIEW: u32 = 1 << 1;
pub const CAMERA_PREVIEW_REASON_RESIZE: u32 = 1 << 2;
pub const CAMERA_PREVIEW_REASON_CAMERA_STOPPED: u32 = 1 << 3;
pub const CAMERA_PREVIEW_REASON_NON_LIVE_SOURCE: u32 = 1 << 4;
pub const CAMERA_PREVIEW_REASON_NON_NV12_MODE: u32 = 1 << 5;
pub const CAMERA_PREVIEW_REASON_NO_CURRENT_FRAME: u32 = 1 << 6;
pub const CAMERA_PREVIEW_REASON_NEW_TIMESTAMP: u32 = 1 << 7;
pub const CAMERA_PREVIEW_REASON_NEW_GENERATION: u32 = 1 << 8;
pub const CAMERA_PREVIEW_REASON_BACKPRESSURE: u32 = 1 << 9;

#[doc(hidden)]
pub fn direct_preview_reason_requires_drawable(reason: u32) -> bool {
    reason != 0 && reason != CAMERA_PREVIEW_REASON_BACKPRESSURE
}

#[doc(hidden)]
pub fn direct_preview_can_reuse_resize_targets(
    current_w: u32,
    current_h: u32,
    current_scale: f32,
    next_w: u32,
    next_h: u32,
    next_scale: f32,
    sample_count: u32,
) -> bool {
    sample_count == 1
        && current_w == next_w
        && current_h == next_h
        && current_scale.to_bits() == next_scale.to_bits()
}

#[doc(hidden)]
pub fn direct_preview_uses_fast_yuv_pipeline(
    bit_depth: i32,
    matrix: i32,
    video_range: i32,
) -> bool {
    bit_depth == 8 && matrix == 0 && (video_range == 0 || video_range == 1)
}

#[doc(hidden)]
pub fn direct_preview_tiny_renderer_active(
    has_tiny_preview_renderer: bool,
    sample_count: u32,
    camera_texture_source: CameraTextureSource,
    camera_render_mode: CameraRenderMode,
) -> bool {
    has_tiny_preview_renderer
        && sample_count == 1
        && camera_texture_source == CameraTextureSource::Live
        && matches!(
            camera_render_mode,
            CameraRenderMode::Nv12Optimized | CameraRenderMode::Nv12Legacy
        )
}

#[doc(hidden)]
pub fn direct_live_preview_needs_render(
    resize_reused: bool,
    has_current_frame: bool,
    current_generation: u64,
    current_timestamp_ns: u64,
    latest_generation: u64,
    latest_timestamp_ns: u64,
) -> u32 {
    if !resize_reused {
        return CAMERA_PREVIEW_REASON_RESIZE;
    }
    if !has_current_frame {
        return CAMERA_PREVIEW_REASON_NO_CURRENT_FRAME;
    }
    if latest_generation > current_generation {
        let mut reason = CAMERA_PREVIEW_REASON_NEW_GENERATION;
        if current_timestamp_ns != 0
            && latest_timestamp_ns != 0
            && latest_timestamp_ns > current_timestamp_ns
        {
            reason |= CAMERA_PREVIEW_REASON_NEW_TIMESTAMP;
        }
        return reason;
    }
    if current_timestamp_ns != 0 && latest_timestamp_ns != 0 {
        if latest_timestamp_ns > current_timestamp_ns {
            return CAMERA_PREVIEW_REASON_NEW_TIMESTAMP;
        }
        return 0;
    }
    0
}

#[inline]
fn camera_aspect_fill_params(
    dest_w: f32,
    dest_h: f32,
    src_w: i32,
    src_h: i32,
) -> ([f32; 2], [f32; 2]) {
    let ar_dest = if dest_h > 0.0 { dest_w / dest_h } else { 1.0 };
    let ar_cam = if src_h > 0 { (src_w as f32) / (src_h as f32) } else { 1.0 };
    let (mut sx, mut sy) = (1.0f32, 1.0f32);
    let (mut bx, mut by) = (0.0f32, 0.0f32);
    if ar_cam > ar_dest {
        sx = ar_dest / ar_cam;
        bx = (1.0 - sx) * 0.5;
    } else if ar_cam < ar_dest {
        sy = ar_cam / ar_dest;
        by = (1.0 - sy) * 0.5;
    }
    ([sx, sy], [bx, by])
}

#[inline]
fn map_solid_vertex_dp_to_clip(
    vertex: api::Vertex,
    viewport_width_dp: f32,
    viewport_height_dp: f32,
) -> api::Vertex {
    let width = viewport_width_dp.max(1.0);
    let height = viewport_height_dp.max(1.0);
    api::Vertex { x: (vertex.x / width) * 2.0 - 1.0, y: 1.0 - (vertex.y / height) * 2.0, ..vertex }
}

#[inline]
fn solid_primitive_for_index_count(index_count: usize) -> Option<MTLPrimitiveType> {
    if index_count < 3 {
        return None;
    }
    if index_count % 3 == 0 {
        Some(MTLPrimitiveType::Triangle)
    } else {
        None
    }
}

#[inline]
fn solid_primitive_for_vertex_count(vertex_count: usize) -> Option<MTLPrimitiveType> {
    if vertex_count < 3 {
        return None;
    }
    if vertex_count == 4 {
        return Some(MTLPrimitiveType::TriangleStrip);
    }
    if vertex_count % 3 == 0 {
        Some(MTLPrimitiveType::Triangle)
    } else {
        None
    }
}

#[inline]
fn normalize_indices_for_local_vertex_span(
    source: &[u16],
    vertex_base: u32,
    vertex_count: u32,
) -> Option<alloc::vec::Vec<u16>> {
    if source.is_empty() {
        return Some(alloc::vec::Vec::new());
    }
    if vertex_count == 0 {
        return None;
    }

    if vertex_count <= u16::MAX as u32 {
        let local_limit = vertex_count as u16;
        let local = source.iter().all(|index| *index < local_limit);
        if local {
            return Some(source.to_vec());
        }
    }

    let vertex_end = vertex_base.saturating_add(vertex_count);
    let mut rebased = alloc::vec::Vec::with_capacity(source.len());
    for index in source.iter().copied() {
        let absolute = index as u32;
        if absolute < vertex_base || absolute >= vertex_end {
            return None;
        }
        rebased.push((absolute - vertex_base) as u16);
    }
    Some(rebased)
}

#[inline]
fn remap_indices_to_span(
    source: &[u16],
    src_vertex_base: u32,
    src_vertex_count: u32,
    dst_vertex_base: u32,
) -> Option<alloc::vec::Vec<u16>> {
    let local = normalize_indices_for_local_vertex_span(source, src_vertex_base, src_vertex_count)?;
    let mut mapped = alloc::vec::Vec::with_capacity(local.len());
    for index in local.iter().copied() {
        let dst = dst_vertex_base.saturating_add(index as u32);
        if dst > u16::MAX as u32 {
            return None;
        }
        mapped.push(dst as u16);
    }
    Some(mapped)
}

fn build_solid_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "solid.vertex", "v_solid")?;
    let f = pipeline_function(lib, "solid.fragment", "f_solid")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(1).unwrap().set_offset(8);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    attrs.object_at(2).unwrap().set_format(MTLVertexFormat::UChar4Normalized);
    attrs.object_at(2).unwrap().set_offset(16);
    attrs.object_at(2).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(20);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.solid.create", &desc)
}

fn build_blur_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "blur.vertex", "v_fullscreen")?;
    let f = pipeline_function(lib, "blur.fragment", "f_blur")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    pipeline_state(device, "pso.blur.create", &desc)
}

fn build_downsample_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "downsample.vertex", "v_fullscreen")?;
    let f = pipeline_function(lib, "downsample.fragment", "f_downsample")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    pipeline_state(device, "pso.downsample.create", &desc)
}

fn build_upsample_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "upsample.vertex", "v_fullscreen")?;
    let f = pipeline_function(lib, "upsample.fragment", "f_upsample")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    pipeline_state(device, "pso.upsample.create", &desc)
}

fn build_backdrop_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "backdrop.vertex", "v_backdrop")?;
    let f = pipeline_function(lib, "backdrop.fragment", "f_backdrop")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.backdrop.create", &desc)
}

fn build_image_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "image.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "image.fragment", "f_image")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.image.create", &desc)
}

fn build_image_single_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "image_single.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "image_single.fragment", "f_image_single")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.image_single.create", &desc)
}

fn build_rrect_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "rrect.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "rrect.fragment", "f_rrect")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.rrect.create", &desc)
}

fn build_nine_slice_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "nine_slice.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "nine_slice.fragment", "f_nine_slice")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.nine_slice.create", &desc)
}

fn build_spinner_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "spinner.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "spinner.fragment", "f_spinner")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.spinner.create", &desc)
}

fn build_text_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    supports_icb: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "text.vertex", "v_text")?;
    let f = pipeline_function(lib, "text.fragment", "f_text")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(1).unwrap().set_offset(8);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    attrs.object_at(2).unwrap().set_format(MTLVertexFormat::UChar4Normalized);
    attrs.object_at(2).unwrap().set_offset(16);
    attrs.object_at(2).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(20);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));
    desc.set_sample_count(sample_count as u64);
    #[cfg(target_os = "ios")]
    if supports_icb {
        desc.set_support_indirect_command_buffers(true);
    }
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.text.create", &desc)
}

fn build_text_sdf_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    supports_icb: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "text_sdf.vertex", "v_text")?;
    let f = pipeline_function(lib, "text_sdf.fragment", "f_text_sdf")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(1).unwrap().set_offset(8);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    attrs.object_at(2).unwrap().set_format(MTLVertexFormat::UChar4Normalized);
    attrs.object_at(2).unwrap().set_offset(16);
    attrs.object_at(2).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(20);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));
    desc.set_sample_count(sample_count as u64);
    #[cfg(target_os = "ios")]
    if supports_icb {
        desc.set_support_indirect_command_buffers(true);
    }
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.text_sdf.create", &desc)
}

fn build_camera_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    fragment_name: &str,
) -> Result<RenderPipelineState, MetalInitError> {
    let stage_vertex = "camera.vertex.v_inst_rect_cam";
    let v = pipeline_function(lib, stage_vertex, "v_inst_rect_cam")?;
    let f = pipeline_function(lib, fragment_name, fragment_name)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    pipeline_state(device, fragment_name, &desc)
}

fn build_sampler(device: &Device) -> Option<SamplerState> {
    let desc = SamplerDescriptor::new();
    desc.set_min_filter(MTLSamplerMinMagFilter::Linear);
    desc.set_mag_filter(MTLSamplerMinMagFilter::Linear);
    // Clamp-to-edge on S/T
    desc.set_address_mode_s(MTLSamplerAddressMode::ClampToEdge);
    desc.set_address_mode_t(MTLSamplerAddressMode::ClampToEdge);
    Some(device.new_sampler(&desc))
}

struct PerFrame {
    cmd: Option<CommandBuffer>,
    submitted: Option<CommandBuffer>,
    in_flight: Arc<AtomicBool>,
    vb_used: usize,
    ib_used: usize,
    ub_used: usize,
}

impl Default for PerFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl PerFrame {
    fn new() -> Self {
        Self {
            cmd: None,
            submitted: None,
            in_flight: Arc::new(AtomicBool::new(false)),
            vb_used: 0,
            ib_used: 0,
            ub_used: 0,
        }
    }
    fn reset(&mut self) {
        self.vb_used = 0;
        self.ib_used = 0;
        self.ub_used = 0;
    }

    fn prepare_for_encode(&mut self) {
        if self.in_flight.load(Ordering::Acquire) {
            if let Some(cmd) = self.submitted.take() {
                cmd.wait_until_completed();
            }
            self.in_flight.store(false, Ordering::Release);
        } else {
            self.submitted = None;
        }
        self.reset();
        self.cmd = None;
    }

    fn mark_submitted(&mut self, cmd: &CommandBuffer) {
        self.in_flight.store(true, Ordering::Release);
        self.submitted = Some(cmd.to_owned());
    }
}

struct Ring {
    bufs: [Buffer; FRAME_RING_SIZE],
    cap: [usize; FRAME_RING_SIZE],
    opts: MTLResourceOptions,
}

impl Ring {
    fn new(device: &Device, initial: usize, opts: MTLResourceOptions) -> Self {
        Self {
            bufs: core::array::from_fn(|_| device.new_buffer(initial as u64, opts)),
            cap: [initial; FRAME_RING_SIZE],
            opts,
        }
    }
    fn ensure_capacity(&mut self, device: &Device, slot: usize, needed: usize) {
        if needed <= self.cap[slot] {
            return;
        }
        let mut new_cap = self.cap[slot] + self.cap[slot] / 2;
        if new_cap < needed {
            new_cap = needed;
        }
        let old = self.bufs[slot].to_owned();
        let old_cap = self.cap[slot];
        let new_buf = device.new_buffer(new_cap as u64, self.opts);
        let copy_len = old_cap.min(new_cap);
        if copy_len > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    old.contents() as *const u8,
                    new_buf.contents() as *mut u8,
                    copy_len,
                );
            }
        }
        self.bufs[slot] = new_buf;
        self.cap[slot] = new_cap;
    }
    fn contents_ptr(&self, slot: usize) -> NonNull<u8> {
        let p = self.bufs[slot].contents();
        NonNull::new(p as *mut u8).expect("non-null")
    }
}

extern crate alloc;

#[derive(Debug)]
struct LayerEntry {
    tex: Texture,
    w: u32,
    h: u32,
    hash: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PerfMemoryStats {
    pub total_bytes: u64,
    pub draw_targets_bytes: u64,
    pub draw_target_main_bytes: u64,
    pub draw_target_msaa_bytes: u64,
    pub effect_targets_bytes: u64,
    pub effect_prepass_bytes: u64,
    pub effect_blur_chain_bytes: u64,
    pub live_camera_bytes: u64,
    pub camera_cache_bytes: u64,
    pub camera_blur_cache_bytes: u64,
    pub camera_transition_cache_bytes: u64,
    pub benchmark_camera_bytes: u64,
    pub layer_cache_bytes: u64,
    pub image_cache_bytes: u64,
    pub buffer_bytes: u64,
    pub pending_command_buffers: u32,
    pub pending_present_drawables: u32,
    pub pending_present_textures: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PerfStats {
    pub memory: PerfMemoryStats,
    pub vb_bytes: u64,
    pub ib_bytes: u64,
    pub ub_bytes: u64,
    pub draws: u32,
    pub instanced: u32,
    pub icb_cmds: u32,
    pub encode_ms: f64,
    pub damage_px: u64,
    pub damage_pct: f32,
    pub damage_rects: u32,
    pub culled: u32,
    // Phase 7 instrumentation
    pub blur_ms: f64,          // time spent updating blurred camera this frame
    pub blur_updates: u32,     // 1 if blurred camera updated this frame, else 0
    pub blur_period_ms: u32,   // current target blur update period
    pub cam_coverage_pct: f32, // fraction of viewport covered by CameraBg
    pub cam_paused: u8,        // 1 if camera paused by adaptive policy
    pub thermal: u8,           // iOS thermal state 0..3 (0 if not iOS)
    pub low_power: u8,         // 1 if Low Power Mode enabled (0 if not iOS)
    pub cam_width: u32,
    pub cam_height: u32,
    pub cam_bit_depth: u8,
    pub cam_matrix: u8,
    pub cam_video_range: u8,
    pub cam_color_space: u8,
    pub cam_poll_submissions_ms: f64,
    pub cam_fetch_ms: f64,
    pub cam_setup_ms: f64,
    pub cam_encode_quad_ms: f64,
    pub cam_command_buffer_ms: f64,
    pub cam_encoder_ms: f64,
    pub cam_encode_bind_ms: f64,
    pub cam_encode_draw_ms: f64,
    pub cam_end_encoding_ms: f64,
    pub cam_present_ms: f64,
    pub cam_commit_ms: f64,
    pub cam_gpu_ms: f64,
    pub preview_submission_depth: u32,
    pub preview_submission_skipped: u32,
    pub preview_submission_frame_age_ms: f64,
}

impl MetalRenderer {
    pub fn last_stats(&self) -> PerfStats {
        self.last_stats
    }

    pub fn set_damage_options(&mut self, enabled: bool, use_thresh: f32, prefilter: f32) {
        self.damage_enabled = enabled;
        self.damage_use_thresh = use_thresh.clamp(0.0, 1.0);
        self.damage_prefilter_thresh = prefilter.clamp(0.0, 1.0);
    }

    fn texture_allocated_bytes(tex: &TextureRef) -> u64 {
        tex.allocated_size() as u64
    }

    fn buffer_allocated_bytes(buf: &BufferRef) -> u64 {
        buf.allocated_size() as u64
    }

    fn push_unique_texture_bytes(seen: &mut HashSet<usize>, total: &mut u64, tex: &TextureRef) {
        let key = tex.as_ptr() as usize;
        if seen.insert(key) {
            *total = total.saturating_add(Self::texture_allocated_bytes(tex));
        }
    }

    fn push_unique_buffer_bytes(seen: &mut HashSet<usize>, total: &mut u64, buf: &BufferRef) {
        let key = buf.as_ptr() as usize;
        if seen.insert(key) {
            *total = total.saturating_add(Self::buffer_allocated_bytes(buf));
        }
    }

    fn unique_texture_category_bytes<'a>(
        seen: &mut HashSet<usize>,
        textures: impl IntoIterator<Item = &'a TextureRef>,
    ) -> u64 {
        let mut total = 0;
        for tex in textures {
            Self::push_unique_texture_bytes(seen, &mut total, tex);
        }
        total
    }

    fn unique_buffer_category_bytes<'a>(
        seen: &mut HashSet<usize>,
        buffers: impl IntoIterator<Item = &'a BufferRef>,
    ) -> u64 {
        let mut total = 0;
        for buf in buffers {
            Self::push_unique_buffer_bytes(seen, &mut total, buf);
        }
        total
    }

    fn memory_stats(&self) -> PerfMemoryStats {
        let mut seen = HashSet::new();
        let draw_target_main_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.target_tex.iter().map(|tex| tex.as_ref()),
        );
        let draw_target_msaa_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.target_msaa_tex.iter().map(|tex| tex.as_ref()),
        );
        let draw_targets_bytes = draw_target_main_bytes.saturating_add(draw_target_msaa_bytes);
        let effect_prepass_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.prepass_tex.iter().map(|tex| tex.as_ref()),
        );
        let effect_blur_chain_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.blur_tmp_tex
                .iter()
                .map(|tex| tex.as_ref())
                .chain(self.half_tex.iter().map(|tex| tex.as_ref()))
                .chain(self.quarter_tex.iter().map(|tex| tex.as_ref()))
                .chain(self.quarter_tmp_tex.iter().map(|tex| tex.as_ref())),
        );
        let effect_targets_bytes = effect_prepass_bytes.saturating_add(effect_blur_chain_bytes);
        let camera_blur_cache_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.cam_blur_tex.iter().map(|tex| tex.as_ref()),
        );
        let camera_transition_cache_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.cam_xfade_prev_tex.iter().map(|tex| tex.as_ref()),
        );
        let camera_cache_bytes =
            camera_blur_cache_bytes.saturating_add(camera_transition_cache_bytes);
        let live_camera_bytes = if let Some(frame) = &self.current_live_camera_frame {
            let y_tex = unsafe { TextureRef::from_ptr(frame.y_tex as *mut MTLTexture) };
            let uv_tex = unsafe { TextureRef::from_ptr(frame.uv_tex as *mut MTLTexture) };
            Self::unique_texture_category_bytes(&mut seen, [y_tex, uv_tex])
        } else {
            0
        };
        let benchmark_camera_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.bench_cam_y_tex
                .iter()
                .map(|tex| tex.as_ref())
                .chain(self.bench_cam_uv_tex.iter().map(|tex| tex.as_ref()))
                .chain(self.bench_cam_bgra_tex.iter().map(|tex| tex.as_ref())),
        );
        let layer_cache_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.layers.values().map(|entry| entry.tex.as_ref()),
        );
        let image_cache_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.images.values().map(|tex| tex.as_ref()),
        );
        let buffer_bytes = Self::unique_buffer_category_bytes(
            &mut seen,
            self.vb
                .bufs
                .iter()
                .map(|buf| buf.as_ref())
                .chain(self.ib.bufs.iter().map(|buf| buf.as_ref()))
                .chain(self.ub.bufs.iter().map(|buf| buf.as_ref()))
                .chain(self.img_arg_buf.iter().map(|buf| buf.as_ref())),
        );
        PerfMemoryStats {
            total_bytes: draw_targets_bytes
                .saturating_add(effect_targets_bytes)
                .saturating_add(live_camera_bytes)
                .saturating_add(camera_cache_bytes)
                .saturating_add(benchmark_camera_bytes)
                .saturating_add(layer_cache_bytes)
                .saturating_add(image_cache_bytes)
                .saturating_add(buffer_bytes),
            draw_targets_bytes,
            draw_target_main_bytes,
            draw_target_msaa_bytes,
            effect_targets_bytes,
            effect_prepass_bytes,
            effect_blur_chain_bytes,
            live_camera_bytes,
            camera_cache_bytes,
            camera_blur_cache_bytes,
            camera_transition_cache_bytes,
            benchmark_camera_bytes,
            layer_cache_bytes,
            image_cache_bytes,
            buffer_bytes,
            pending_command_buffers: self
                .camera_preview_renderer
                .as_ref()
                .map(CameraPreviewRenderer::pending_submission_count)
                .unwrap_or(self.direct_preview_submitted.len() as u32),
            pending_present_drawables: self.pending_present_drawable as u32,
            pending_present_textures: self.pending_present_texture as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_param_layouts_match_metal_contracts() {
        use core::mem::{align_of, size_of};

        assert_eq!(align_of::<NineSliceGpuParams>(), 16);
        assert_eq!(size_of::<NineSliceGpuParams>(), 64);

        assert_eq!(align_of::<SpinnerGpuParams>(), 8);
        assert_eq!(size_of::<SpinnerGpuParams>(), 24);

        assert_eq!(align_of::<ImageGpuParams>(), 16);
        assert_eq!(size_of::<ImageGpuParams>(), 48);

        assert_eq!(align_of::<CameraGpuParams>(), 16);
        assert_eq!(size_of::<CameraGpuParams>(), 80);
    }

    #[test]
    fn solid_vertex_dp_maps_to_clip_space() {
        let center = api::Vertex { x: 201.0, y: 437.0, u: 0.0, v: 0.0, rgba: 0xAABBCCDD };
        let mapped_center = map_solid_vertex_dp_to_clip(center, 402.0, 874.0);
        assert!((mapped_center.x - 0.0).abs() < 1e-4);
        assert!((mapped_center.y - 0.0).abs() < 1e-4);
        assert_eq!(mapped_center.rgba, center.rgba);

        let top_left = api::Vertex { x: 0.0, y: 0.0, u: 0.5, v: 0.25, rgba: 0x01020304 };
        let mapped_top_left = map_solid_vertex_dp_to_clip(top_left, 402.0, 874.0);
        assert!((mapped_top_left.x + 1.0).abs() < 1e-4);
        assert!((mapped_top_left.y - 1.0).abs() < 1e-4);
        assert_eq!(mapped_top_left.u, top_left.u);
        assert_eq!(mapped_top_left.v, top_left.v);

        let bottom_right = api::Vertex { x: 402.0, y: 874.0, u: 1.0, v: 1.0, rgba: 0xFFFFFFFF };
        let mapped_bottom_right = map_solid_vertex_dp_to_clip(bottom_right, 402.0, 874.0);
        assert!((mapped_bottom_right.x - 1.0).abs() < 1e-4);
        assert!((mapped_bottom_right.y + 1.0).abs() < 1e-4);
    }

    #[test]
    fn indexed_solid_requires_triangle_multiple() {
        assert_eq!(solid_primitive_for_index_count(3), Some(MTLPrimitiveType::Triangle));
        assert_eq!(solid_primitive_for_index_count(6), Some(MTLPrimitiveType::Triangle));
        assert_eq!(solid_primitive_for_index_count(4), None);
        assert_eq!(solid_primitive_for_index_count(5), None);
    }

    #[test]
    fn nonindexed_solid_allows_triangles_and_quads_only() {
        assert_eq!(solid_primitive_for_vertex_count(3), Some(MTLPrimitiveType::Triangle));
        assert_eq!(solid_primitive_for_vertex_count(4), Some(MTLPrimitiveType::TriangleStrip));
        assert_eq!(solid_primitive_for_vertex_count(6), Some(MTLPrimitiveType::Triangle));
        assert_eq!(solid_primitive_for_vertex_count(5), None);
        assert_eq!(solid_primitive_for_vertex_count(7), None);
    }

    #[test]
    fn normalize_indices_accepts_local_indices() {
        let source = [0_u16, 1, 2, 2, 1, 3];
        let normalized = normalize_indices_for_local_vertex_span(&source, 12, 4)
            .expect("normalize local indices");
        assert_eq!(normalized, source);
    }

    #[test]
    fn normalize_indices_rebases_global_indices() {
        let source = [12_u16, 13, 14, 14, 13, 15];
        let normalized = normalize_indices_for_local_vertex_span(&source, 12, 4)
            .expect("normalize global indices");
        assert_eq!(normalized, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn normalize_indices_rejects_out_of_range_indices() {
        let source = [0_u16, 1, 2, 9];
        let normalized = normalize_indices_for_local_vertex_span(&source, 4, 4);
        assert!(normalized.is_none());
    }

    #[test]
    fn normalize_indices_handles_large_vertex_spans_without_u16_wrap() {
        let source = [0_u16, 1, 2, 2, 1, 3];
        let normalized = normalize_indices_for_local_vertex_span(&source, 70_000, 70_000);
        assert!(normalized.is_none());
    }

    #[test]
    fn remap_indices_to_span_accepts_local_source() {
        let source = [0_u16, 1, 2, 2, 1, 3];
        let remapped =
            remap_indices_to_span(&source, 12, 4, 40).expect("remap local indices to span");
        assert_eq!(remapped, vec![40, 41, 42, 42, 41, 43]);
    }

    #[test]
    fn remap_indices_to_span_accepts_global_source() {
        let source = [12_u16, 13, 14, 14, 13, 15];
        let remapped =
            remap_indices_to_span(&source, 12, 4, 40).expect("remap global indices to span");
        assert_eq!(remapped, vec![40, 41, 42, 42, 41, 43]);
    }

    #[test]
    fn remap_indices_to_span_rejects_out_of_range_source() {
        let source = [12_u16, 13, 44];
        let remapped = remap_indices_to_span(&source, 12, 4, 40);
        assert!(remapped.is_none());
    }

    #[test]
    fn set_bytes_chunking_respects_metal_limit() {
        // Image params: 16B vertex rect + 48B fragment params => 85 instances max per chunk.
        let max = max_instances_per_set_bytes(16, 48);
        assert_eq!(max, 85);
        assert!(max.saturating_mul(16) <= METAL_SET_BYTES_LIMIT);
        assert!(max.saturating_mul(48) <= METAL_SET_BYTES_LIMIT);

        // Spinner params: 16B vertex rect + 24B fragment params => 170 instances max per chunk.
        let spinner = max_instances_per_set_bytes(16, 24);
        assert_eq!(spinner, 170);
        assert!(spinner.saturating_mul(16) <= METAL_SET_BYTES_LIMIT);
        assert!(spinner.saturating_mul(24) <= METAL_SET_BYTES_LIMIT);
    }

    #[test]
    fn simulator_safety_overrides_disable_optional_fast_paths() {
        assert!(!apply_simulator_safety_bool(true, true));
        assert!(!apply_simulator_safety_bool(true, false));
        assert!(apply_simulator_safety_bool(false, true));
        assert!(!apply_simulator_safety_bool(false, false));
    }

    #[test]
    fn simulator_safety_overrides_force_non_hdr_single_sample() {
        assert_eq!(apply_simulator_sample_count(true, 4), 1);
        assert_eq!(apply_simulator_sample_count(true, 1), 1);
        assert_eq!(apply_simulator_sample_count(false, 0), 1);
        assert_eq!(apply_simulator_sample_count(false, 4), 4);
        assert!(!apply_simulator_hdr(true, true));
        assert!(!apply_simulator_hdr(true, false));
        assert!(apply_simulator_hdr(false, true));
    }

    #[test]
    fn simulator_detection_matches_compile_target() {
        if cfg!(target_os = "ios") && cfg!(target_abi = "sim") {
            assert!(running_on_ios_simulator());
        }
    }

    #[cfg(all(target_os = "ios", target_abi = "sim"))]
    #[test]
    fn simulator_defaults_disable_icb_and_layer_cache() {
        assert!(!glyph_icb_enabled_default());
        assert!(!layer_cache_enabled_default());
    }

    #[test]
    fn glyph_icb_cpu_recording_uses_shared_storage() {
        assert_eq!(glyph_icb_resource_options(), MTLResourceOptions::StorageModeShared);
    }

    #[cfg(all(target_os = "ios", not(target_abi = "sim")))]
    #[test]
    fn ios_device_defaults_enable_icb_and_layer_cache_without_simulator_udid() {
        if std::env::var_os("SIMULATOR_UDID").is_some() {
            return;
        }
        assert!(glyph_icb_enabled_default());
        assert!(layer_cache_enabled_default());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_defaults_disable_icb_but_keep_layer_cache() {
        assert!(!glyph_icb_enabled_default());
        assert!(layer_cache_enabled_default());
    }

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn ring_resizes_buffers() {
        let Some(device) = Device::system_default() else { return };
        let mut ring = Ring::new(&device, 128, MTLResourceOptions::StorageModeShared);
        let initial = ring.cap[0];
        ring.ensure_capacity(&device, 0, initial * 4);
        assert!(ring.cap[0] >= initial * 4);
        assert!(!ring.contents_ptr(0).as_ptr().is_null());
    }

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn ring_resizes_preserve_buffer_prefix_data() {
        let Some(device) = Device::system_default() else { return };
        let mut ring = Ring::new(&device, 64, MTLResourceOptions::StorageModeShared);
        let seed: [u8; 32] = [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
            0xF0, 0x0F, 0x13, 0x37, 0x42, 0x24, 0x7E, 0xE7, 0x5A, 0xA5, 0xC3, 0x3C, 0x18, 0x81,
            0x2D, 0xD2, 0x4B, 0xB4,
        ];
        unsafe {
            core::ptr::copy_nonoverlapping(
                seed.as_ptr(),
                ring.contents_ptr(0).as_ptr(),
                seed.len(),
            );
        }
        ring.ensure_capacity(&device, 0, 1024);
        let grown =
            unsafe { core::slice::from_raw_parts(ring.contents_ptr(0).as_ptr(), seed.len()) };
        assert_eq!(grown, &seed);
    }

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn renderer_initial_stats_zero() {
        match MetalRenderer::new_default() {
            Ok(renderer) => {
                let stats = renderer.last_stats();
                assert_eq!(stats.draws, 0);
                assert_eq!(stats.damage_rects, 0);
            }
            Err(MetalInitError::NoDevice) => {}
            Err(e) => panic!("unexpected init error: {e}"),
        }
    }
}
