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

pub mod id_mask_compositor;
pub mod neon_marker;
pub mod scene3d;

mod id_mask_gpu;
mod neon_marker_gpu;
mod prepared;

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
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use thiserror::Error;

static NEXT_DEVICE_GENERATION: AtomicU64 = AtomicU64::new(1);

const LAYER_CACHE_MIN_BUDGET_BYTES: u64 = 32 * 1024 * 1024;
const LAYER_CACHE_MAX_BUDGET_BYTES: u64 = 256 * 1024 * 1024;
const LAYER_CACHE_POOL_BUDGET_DIVISOR: u64 = 4;
const LAYER_CACHE_ABSENT_FRAMES: u64 = 120;
const LAYER_CACHE_POOL_MAX_AGE_FRAMES: u64 = 60;

const ID_MASK_CACHE_MIN_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
const ID_MASK_CACHE_MAX_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const ID_MASK_CACHE_MAX_ENTRIES: usize = 4;

const LAYER_PURGE_NONE: u8 = 0;
const LAYER_PURGE_EXPLICIT: u8 = 1;
const LAYER_PURGE_MEMORY_WARNING: u8 = 2;
const LAYER_PURGE_DEVICE_LOSS: u8 = 3;
const LAYER_PURGE_SCALE_CHANGE: u8 = 4;

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
        env_flag("OXIDE_RUST_LOG").unwrap_or(false)
            || env_flag("NAMETAG_DEBUG_RUNTIME_CREATE").unwrap_or(false)
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
fn renderer_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env_flag("OXIDE_RENDERER_TRACE").unwrap_or(false)
            || env_flag("NAMETAG_DRAW_TIMING").unwrap_or(false)
            || env_flag("NAMETAG_DRAW_TRACE").unwrap_or(false)
    })
}

#[inline(always)]
fn renderer_trace_log(msg: &str) {
    #[cfg(target_os = "ios")]
    unsafe {
        oxide_host_ios_log(msg.as_ptr() as *const core::ffi::c_char, msg.len());
    }
    #[cfg(not(target_os = "ios"))]
    {
        eprintln!("{}", msg);
    }
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn camera_perf_trace_signposts_enabled() -> bool {
    #[cfg(target_os = "ios")]
    {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        cached_env_flag(&ENABLED, "OXIDE_PERF_CAMERA_TRACE_PHASES")
    }
    #[cfg(not(target_os = "ios"))]
    {
        false
    }
}

#[inline(always)]
fn camera_perf_stage_stats_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    cached_env_flag(&ENABLED, "OXIDE_PERF_PARKED")
}

#[inline(always)]
fn experimental_tiny_camera_preview_renderer_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    cached_env_flag(&ENABLED, "OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER")
}

#[inline(always)]
fn experimental_preview_submission_backpressure_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    cached_env_flag(&ENABLED, "OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE")
}

#[inline(always)]
pub fn direct_preview_uses_dontcare_load_action() -> bool {
    env_flag("OXIDE_PERF_CAMERA_PREVIEW_DONT_CARE_LOAD").unwrap_or(false)
}

#[inline(always)]
fn experimental_preview_submission_cap() -> Option<usize> {
    static CAP: OnceLock<Option<usize>> = OnceLock::new();
    *CAP.get_or_init(|| {
        if let Ok(value) = std::env::var("OXIDE_PERF_CAMERA_PREVIEW_SUBMISSION_CAP") {
            if let Ok(parsed) = value.trim().parse::<usize>() {
                if parsed >= 1 {
                    return Some(parsed);
                }
            }
        }
        if experimental_preview_submission_backpressure_enabled() {
            Some(2)
        } else {
            None
        }
    })
}

#[inline(always)]
pub fn direct_preview_submission_backpressure_applies(
    submission_cap: Option<usize>,
    in_flight: usize,
) -> bool {
    submission_cap.is_some_and(|limit| in_flight >= limit)
}

#[inline(always)]
pub fn direct_preview_should_clear_load_action(
    dontcare_load_enabled: bool,
    draws_live_frame: bool,
) -> bool {
    !dontcare_load_enabled || !draws_live_frame
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

#[cfg(target_os = "ios")]
fn mark_preview_generation_presented(generation: u64) {
    if generation == 0 {
        return;
    }
    unsafe extern "C" {
        fn oxide_cam_mark_presented_generation(generation: u64);
    }
    unsafe {
        oxide_cam_mark_presented_generation(generation);
    }
}

#[cfg(not(target_os = "ios"))]
fn mark_preview_generation_presented(_generation: u64) {}

#[inline(always)]
fn elapsed_ms(start: Option<Instant>) -> f64 {
    start.map(|value| value.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
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

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
#[inline(always)]
unsafe fn command_buffer_gpu_duration_ms(_buffer: &CommandBufferRef) -> f64 {
    0.0
}

#[cfg(target_os = "ios")]
#[inline(always)]
fn render_pass_gpu_stage_timestamps_enabled() -> bool {
    env_flag("OXIDE_PERF_GPU_TIMESTAMPS")
        .or_else(|| env_flag("OXIDE_PERF_CAMERA_GPU_TIMESTAMPS"))
        .unwrap_or_else(|| env_flag("OXIDE_PERF_PARKED").unwrap_or(false))
}

#[cfg(target_os = "ios")]
#[inline(always)]
fn gpu_timestamp_interval_ms(
    begin: u64,
    end: u64,
    cpu_start: u64,
    cpu_end: u64,
    gpu_start: u64,
    gpu_end: u64,
) -> f64 {
    if end <= begin || cpu_end <= cpu_start || gpu_end <= gpu_start {
        return 0.0;
    }
    let sample_span = (end - begin) as f64;
    let cpu_span = (cpu_end - cpu_start) as f64;
    let gpu_span = (gpu_end - gpu_start) as f64;
    if sample_span <= 0.0 || cpu_span <= 0.0 || gpu_span <= 0.0 {
        return 0.0;
    }
    let nanos = sample_span / gpu_span * cpu_span;
    if nanos.is_finite() && nanos > 0.0 {
        nanos / 1_000_000.0
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct GpuStageStats {
    render_ms: f64,
    vertex_ms: f64,
    fragment_ms: f64,
}

#[cfg(target_os = "ios")]
#[derive(Clone)]
struct GpuStageTimingSupport {
    counter_set: CounterSet,
}

#[cfg(not(target_os = "ios"))]
#[derive(Clone)]
struct GpuStageTimingSupport;

#[cfg(target_os = "ios")]
#[derive(Clone)]
struct GpuStageTrace {
    sample_buffer: CounterSampleBuffer,
    cpu_start: u64,
    gpu_start: u64,
}

#[cfg(not(target_os = "ios"))]
#[derive(Clone)]
struct GpuStageTrace;

#[derive(Clone, Copy, Debug, Default)]
struct CompletedGpuStats {
    frame_id: u64,
    command_ms: f64,
    render_ms: f64,
    vertex_ms: f64,
    fragment_ms: f64,
}

#[cfg(target_os = "ios")]
impl GpuStageTimingSupport {
    fn new(device: &Device) -> Option<Self> {
        if !render_pass_gpu_stage_timestamps_enabled()
            || !device.supports_counter_sampling(MTLCounterSamplingPoint::AtStageBoundary)
        {
            return None;
        }
        let counter_set =
            device.counter_sets().into_iter().find(|set| set.name() == "timestamp")?;
        Some(Self { counter_set })
    }

    fn begin_submission(&self, device: &Device) -> Option<GpuStageTrace> {
        let desc = CounterSampleBufferDescriptor::new();
        desc.set_storage_mode(MTLStorageMode::Shared);
        desc.set_sample_count(4);
        desc.set_counter_set(&self.counter_set);
        let sample_buffer = device.new_counter_sample_buffer_with_descriptor(&desc).ok()?;
        let mut cpu_start = 0;
        let mut gpu_start = 0;
        device.sample_timestamps(&mut cpu_start, &mut gpu_start);
        Some(GpuStageTrace { sample_buffer, cpu_start, gpu_start })
    }
}

#[cfg(target_os = "ios")]
impl GpuStageTrace {
    fn configure_render_pass(&self, descriptor: &RenderPassDescriptorRef) {
        let sample_attachment = descriptor.sample_buffer_attachments().object_at(0).unwrap();
        sample_attachment.set_sample_buffer(&self.sample_buffer);
        sample_attachment.set_start_of_vertex_sample_index(0);
        sample_attachment.set_end_of_vertex_sample_index(1);
        sample_attachment.set_start_of_fragment_sample_index(2);
        sample_attachment.set_end_of_fragment_sample_index(3);
    }

    fn resolve(&self, device: &Device) -> GpuStageStats {
        let mut cpu_end = 0;
        let mut gpu_end = 0;
        device.sample_timestamps(&mut cpu_end, &mut gpu_end);
        let Some(samples) = (unsafe { resolve_gpu_timestamp_samples(&self.sample_buffer) }) else {
            return GpuStageStats::default();
        };
        let vertex_ms = gpu_timestamp_interval_ms(
            samples[0],
            samples[1],
            self.cpu_start,
            cpu_end,
            self.gpu_start,
            gpu_end,
        );
        let fragment_ms = gpu_timestamp_interval_ms(
            samples[2],
            samples[3],
            self.cpu_start,
            cpu_end,
            self.gpu_start,
            gpu_end,
        );
        GpuStageStats { render_ms: vertex_ms + fragment_ms, vertex_ms, fragment_ms }
    }
}

#[cfg(not(target_os = "ios"))]
impl GpuStageTimingSupport {
    fn new(_device: &Device) -> Option<Self> {
        None
    }

    fn begin_submission(&self, _device: &Device) -> Option<GpuStageTrace> {
        None
    }
}

#[cfg(not(target_os = "ios"))]
impl GpuStageTrace {
    fn configure_render_pass(&self, _descriptor: &RenderPassDescriptorRef) {}

    fn resolve(&self, _device: &Device) -> GpuStageStats {
        GpuStageStats::default()
    }
}

#[cfg(target_os = "ios")]
unsafe fn resolve_gpu_timestamp_samples(
    sample_buffer: &CounterSampleBufferRef,
) -> Option<[u64; 4]> {
    let ns_data: *mut Object = msg_send![
        sample_buffer.as_ptr(),
        resolveCounterRange: NSRange::new(0u64, 4u64)
    ];
    if ns_data.is_null() {
        return None;
    }
    let length: NSUInteger = msg_send![ns_data, length];
    let bytes: *const std::ffi::c_void = msg_send![ns_data, bytes];
    let expected_bytes = core::mem::size_of::<u64>() * 4;
    if bytes.is_null() || (length as usize) < expected_bytes {
        return None;
    }
    let resolved = std::slice::from_raw_parts(bytes.cast::<u64>(), 4);
    Some([resolved[0], resolved[1], resolved[2], resolved[3]])
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
unsafe fn command_buffer_error_detail(buffer: &CommandBufferRef) -> Option<String> {
    let err: *mut Object = msg_send![buffer, error];
    if err.is_null() {
        return None;
    }
    let code: i64 = msg_send![err, code];
    let domain_obj: *mut Object = msg_send![err, domain];
    let desc_obj: *mut Object = msg_send![err, localizedDescription];
    let domain = nsstring_to_string(domain_obj).unwrap_or_else(|| "<null-domain>".to_string());
    let desc = nsstring_to_string(desc_obj).unwrap_or_else(|| "<null-description>".to_string());
    Some(format!("domain={} code={} desc={}", domain, code, desc))
}

#[inline(always)]
fn env_flag(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|value| {
        matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    })
}

#[inline(always)]
fn cached_env_flag(cache: &OnceLock<bool>, name: &str) -> bool {
    *cache.get_or_init(|| env_flag(name).unwrap_or(false))
}

#[inline(always)]
fn transparent_drawable_clear_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env_flag("OXIDE_METAL_TRANSPARENT_DRAWABLE").unwrap_or(false)
            || env_flag("NAMETAG_NATIVE_CAMERA_TRANSPARENT_METAL").unwrap_or(false)
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
    std::env::var("OXIDE_DEBUG_ENCODE_EVERY")
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
        api::DrawCmd::ImageMesh { .. } => "image_mesh",
        api::DrawCmd::GlyphRun { .. } => "glyph_run",
        api::DrawCmd::RRect { .. } => "rrect",
        api::DrawCmd::NineSlice { .. } => "nine_slice",
        api::DrawCmd::Backdrop { .. } => "backdrop",
        api::DrawCmd::VisualEffect { .. } => "visual_effect",
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
fn layer_cache_enabled_default() -> bool {
    // Layer texture caching has exhibited stale/blank composition on Simulator.
    // Prefer deterministic inline layer rendering there unless explicitly enabled.
    if running_on_ios_simulator() {
        return false;
    }
    true
}

const VISIBLE_FRAME_RESOURCE_DEPTH: usize = 3;
const OFFSCREEN_FRAME_RESOURCE_DEPTH: usize = 8;
const MAX_FRAME_RESOURCE_DEPTH: usize = OFFSCREEN_FRAME_RESOURCE_DEPTH;
const INITIAL_VERTEX_BUFFER_BYTES: usize = 512 * 1024;
const INITIAL_INDEX_BUFFER_BYTES: usize = 64 * 1024;
const INITIAL_UNIFORM_BUFFER_BYTES: usize = 72 * 1024;
const INITIAL_PROPERTY_BUFFER_BYTES: usize = 16 * 1024;
const IMAGE_ARG_TEXTURE_SLOTS: u32 = 128;
const IMAGE_ARG_SMALL_TABLE_COUNT: usize = 8;
const LEGACY_SPINNER_LARGE_ATOM: f32 = 37.0;
const LEGACY_SPINNER_LARGE_STROKE: f32 = 2.5;
const LEGACY_SPINNER_ROTATION_MS: u64 = 1_000;

#[inline]
fn align_up_usize(value: usize, alignment: usize) -> usize
{
    (value.saturating_add(alignment.saturating_sub(1)) / alignment).saturating_mul(alignment)
}

#[inline]
fn image_argument_table_key(handles: &[u32]) -> u64
{
    let mut key = 0xcbf2_9ce4_8422_2325_u64 ^ handles.len() as u64;
    for handle in handles
    {
        key ^= *handle as u64;
        key = key.wrapping_mul(0x0000_0100_0000_01b3);
    }
    key
}

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

const DEFAULT_METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/default.metallib"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetalRendererConfig {
    pub wants_hdr: bool,
    pub sample_count: u32,
    pub camera_render_mode: CameraRenderMode,
    pub camera_texture_source: CameraTextureSource,
    pub direct_preview_only: bool,
    pub frame_resource_depth: usize,
}

#[cfg(feature = "snapshot-tests")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetalSnapshotColorFormat
{
   Bgra8Srgb,
   Bgra10Xr,
}

#[cfg(feature = "snapshot-tests")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetalSnapshotColorReadback
{
   pub width: u32,
   pub height: u32,
   pub format: MetalSnapshotColorFormat,
   pub bytes: alloc::vec::Vec<u8>,
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
            frame_resource_depth: OFFSCREEN_FRAME_RESOURCE_DEPTH,
        }
    }
}

impl MetalRendererConfig
{
   pub fn visible_host() -> Self
   {
      Self {
         frame_resource_depth: VISIBLE_FRAME_RESOURCE_DEPTH,
         ..Self::default()
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

#[derive(Clone)]
struct DirectPreviewSubmittedFrame {
    frame_id: u64,
    generation: u64,
    cmd: CommandBuffer,
    gpu_trace: Option<GpuStageTrace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeshFormat3d {
    Position,
    PositionColor,
}

struct Mesh3dGpu {
    vb: Buffer,
    ib: Buffer,
    index_count: u64,
    topology: scene3d::MeshTopology,
    format: MeshFormat3d,
    opaque: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Scene3dGpuUniforms {
    mvp: scene3d::Mat4,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Scene3dGpuMaterial {
    color: [f32; 4],
    material: u32,
    _pad: [f32; 3],
    params: [f32; 4],
}

const _: () = assert!(core::mem::size_of::<Scene3dGpuUniforms>() == 64);
const _: () = assert!(core::mem::size_of::<Scene3dGpuMaterial>() == 48);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scene3dPipelineKind {
    PositionTriAlpha,
    PositionTriAdditive,
    PositionTriDepth,
    PositionTriBloom,
    PositionLineAlpha,
    PositionLineAdditive,
    PositionLineDepth,
    PositionLineBloom,
    ColorTriAlpha,
    ColorTriAdditive,
    ColorTriDepth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scene3dDepthKind {
    Disabled,
    Read,
    Write,
    WriteNoTest,
}

#[derive(Clone, Copy)]
struct Scene3dDraw {
    mesh: u32,
    first_instance: u32,
    instance_count: u32,
    pipeline: Scene3dPipelineKind,
    depth: Scene3dDepthKind,
    cull: scene3d::CullMode3d,
    batchable: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct GlyphGpuInstance
{
   dst: [f32; 4],
   uv: [f32; 4],
   color: [f32; 4],
}

const _: () = assert!(core::mem::size_of::<GlyphGpuInstance>() == 48);

#[allow(dead_code)]
pub struct MetalRenderer {
    device: Device,
    device_generation: u64,
    queue: CommandQueue,
    pso_solid: RenderPipelineState,
    pso_image: RenderPipelineState,
    pso_layer_image: RenderPipelineState,
    pso_image_single: RenderPipelineState,
    pso_layer_image_single: RenderPipelineState,
    pso_image_mesh: RenderPipelineState,
    pso_layer_image_mesh: RenderPipelineState,
    pso_blur: RenderPipelineState,
    pso_blur_paired: RenderPipelineState,
    pso_downsample: RenderPipelineState,
    pso_upsample: RenderPipelineState,
    pso_backdrop: RenderPipelineState,
    pso_visual_effect: RenderPipelineState,
    pso_rrect: RenderPipelineState,
    pso_layer_rrect: RenderPipelineState,
    pso_nine_slice: RenderPipelineState,
    pso_layer_nine_slice: RenderPipelineState,
    pso_layer_composite: RenderPipelineState,
    pso_layer_composite_aligned: RenderPipelineState,
    pso_spinner: RenderPipelineState,
    pso_layer_spinner: RenderPipelineState,
    pso_text: RenderPipelineState,
    pso_layer_text: RenderPipelineState,
    pso_text_sdf: RenderPipelineState,
    pso_layer_text_sdf: RenderPipelineState,
    prepared_pipelines: Option<prepared::PreparedPipelines>,
    pso_camera: RenderPipelineState,
    pso_camera_legacy: RenderPipelineState,
    pso_camera_preview_fast_full: RenderPipelineState,
    pso_camera_preview_fast_video: RenderPipelineState,
    pso_camera_bgra: RenderPipelineState,
    pso_scene3d_tri: RenderPipelineState,
    pso_scene3d_tri_depth: RenderPipelineState,
    pso_scene3d_tri_add: RenderPipelineState,
    pso_scene3d_tri_add_bloom: RenderPipelineState,
    pso_scene3d_color_tri: RenderPipelineState,
    pso_scene3d_color_tri_add: RenderPipelineState,
    pso_scene3d_color_tri_depth: RenderPipelineState,
    pso_scene3d_line: RenderPipelineState,
    pso_scene3d_line_depth: RenderPipelineState,
    pso_scene3d_line_add: RenderPipelineState,
    pso_scene3d_line_add_bloom: RenderPipelineState,
    pso_bloom_blur: RenderPipelineState,
    pso_bloom_blur_paired: RenderPipelineState,
    pso_bloom_composite: RenderPipelineState,
    pso_id_mask_raster: RenderPipelineState,
    pso_id_mask_field_seed: RenderPipelineState,
    pso_id_mask_field_seed_wide: RenderPipelineState,
    pso_id_mask_field_jump: RenderPipelineState,
    pso_id_mask_field_jump_wide: RenderPipelineState,
    pso_id_mask_compositor: RenderPipelineState,
    pso_id_mask_compositor_wide: RenderPipelineState,
    pso_neon_marker: RenderPipelineState,
    depth_state_3d_disabled: DepthStencilState,
    depth_state_3d_read: DepthStencilState,
    depth_state_3d_write: DepthStencilState,
    depth_state_3d_write_no_test: DepthStencilState,
    prepared_layer_pipelines: Option<prepared::PreparedPipelines>,
    prepared_exact_layer_pipelines: Option<prepared::PreparedPipelines>,
    // Argument buffer for image textures
    img_arg: Option<ArgumentEncoder>,
    img_arg_bufs: Option<Ring>,
    img_arg_stride: usize,
    img_arg_used: usize,
    image_arg_tables: alloc::vec::Vec<ImageArgTable>,
    image_arg_table_index: HashMap<u64, usize>,
    image_arg_table_count: usize,
    image_arg_handles: alloc::vec::Vec<u32>,
    image_vbuf: alloc::vec::Vec<f32>,
    image_fbuf: alloc::vec::Vec<ImageGpuParams>,
    effect_fbuf: alloc::vec::Vec<f32>,
    effect_graph_events: alloc::vec::Vec<api::EffectGraphEvent>,
    effect_graph_plan: api::EffectGraphPlan,
    effect_graph_key: u64,
    #[cfg(feature = "snapshot-tests")]
    force_exact_blur_for_snapshot: bool,
    sampler: Option<SamplerState>,
    color_format: MTLPixelFormat,
    config: MetalRendererConfig,
    sample_count: u32,
    hdr_enabled: bool,
    frame_id: u64,
    frame_slot: usize,
    frame_backpressure_skipped: bool,
    frame_in_flight: Arc<AtomicU8>,
    frames: alloc::vec::Vec<PerFrame>,
    vb: Ring,
    ib: Ring,
    ub: Ring,
    property_ring: Ring,
    image_tex_map: HashMap<u32, u32>,
    glyph_instances: alloc::vec::Vec<GlyphGpuInstance>,
    filtered_prepass: FilteredDrawList,
    filtered_main: FilteredDrawList,
    layer_plans: alloc::vec::Vec<LayerPlan>,
    layer_plan_stack: alloc::vec::Vec<LayerPlanStackEntry>,
    layer_sublist: api::DrawList,
    layer_scratch_frame: Option<PerFrame>,
    clip_stack_pool: alloc::vec::Vec<alloc::vec::Vec<api::RectI>>,
    target_w: u32,
    target_h: u32,
    target_scale: f32,
    target_tex: Option<Texture>,
    target_msaa_tex: Option<Texture>,
    depth_tex: Option<Texture>,
    prepass_tex: Option<Texture>,
    half_tex: Option<Texture>,
    quarter_tex: Option<Texture>,
    quarter_tmp_tex: Option<Texture>,
    eighth_tex: Option<Texture>,
    eighth_tmp_tex: Option<Texture>,
    scene3d_bloom_tex: Option<Texture>,
    scene3d_bloom_tmp_tex: Option<Texture>,
    id_mask_snapshot_target: Option<id_mask_gpu::RenderTargets>,
    id_mask_in_flight_generations: alloc::vec::Vec<alloc::vec::Vec<id_mask_gpu::IdMaskInFlightGeneration>>,
    id_mask_field_cache: alloc::vec::Vec<id_mask_gpu::IdMaskFieldCacheEntry>,
    id_mask_frame_cache_serials: alloc::vec::Vec<u64>,
    id_mask_cache_budget_bytes: u64,
    id_mask_cache_resident_bytes: u64,
    id_mask_cache_evictions: u64,
    next_id_mask_cache_serial: u64,
    id_mask_target_peak_bytes: u64,
    id_mask_target_reuse_blocked: u64,
    id_mask_vertex_caches: alloc::vec::Vec<IdMaskVertexUploadCache>,
    images: HashMap<u32, Texture>,
    image_generations: HashMap<u32, u64>,
    next_image_id: u32,
    prepared_chunks: prepared::PreparedChunkCache,
    prepared_property_cache: prepared::PreparedPropertyCache,
    prepared_frame_plan: alloc::vec::Vec<prepared::PreparedFrameInstance>,
    prepared_frame_snapshot: Option<api::RenderSnapshot>,
    prepared_frame_viewport: [f32; 2],
    prepared_frame_keys: alloc::vec::Vec<prepared::PreparedChunkKey>,
    prepared_layer_frame_keys: HashMap<u32, prepared::PreparedLayerKey>,
    prepared_damage_instances: alloc::vec::Vec<u32>,
    prepared_damage_commands: alloc::vec::Vec<u32>,
    prepared_fallback: api::DrawList,
    meshes_3d: HashMap<u32, Mesh3dGpu>,
    next_mesh3d_id: u32,
    scene3d_draws: alloc::vec::Vec<Scene3dDraw>,
    layers: HashMap<u32, LayerEntry>,
    layer_pool: alloc::vec::Vec<LayerPoolEntry>,
    layer_frame_ids: HashSet<u32>,
    layer_cache_budget_bytes: u64,
    layer_cache_resident_bytes: u64,
    layer_cache_pool_bytes: u64,
    layer_cache_pool_reuses: u64,
    layer_cache_evictions: u64,
    layer_cache_recreations: u64,
    layer_cache_purges: u64,
    layer_cache_last_purge_reason: u8,
    layer_cache_enabled: bool,
    encoding_layer: bool,
    inline_layer_counter_active: bool,
    last_stats: PerfStats,
    acc_draws: u32,
    acc_flat_instanced_draws: u32,
    acc_instanced: u32,
    acc_analytic_instance_bytes: u64,
    acc_analytic_instance_buffer_binds: u32,
    acc_analytic_instance_ring_grows: u32,
    acc_scene3d_draws: u32,
    acc_scene3d_instances: u32,
    acc_scene3d_instance_bytes: u64,
    acc_scene3d_pipeline_binds: u32,
    acc_scene3d_depth_state_binds: u32,
    acc_scene3d_cull_sets: u32,
    acc_scene3d_mesh_buffer_binds: u32,
    acc_scene3d_instance_buffer_binds: u32,
    acc_scene3d_instance_ring_grows: u32,
    acc_scene3d_viewport_sets: u32,
    acc_glyph_instance_bytes: u64,
    acc_glyph_instance_buffer_binds: u32,
    acc_glyph_instances: u32,
    acc_icb_cmds: u32,
    acc_commands_traversed: u64,
    acc_commands_copied: u64,
    acc_geometry_bytes_copied: u64,
    acc_chunks_reused: u64,
    acc_chunks_rebuilt: u64,
    acc_chunks_prepared: u64,
    acc_prepared_plan_reuses: u64,
    acc_backend_cache_hits: u64,
    acc_backend_cache_misses: u64,
    acc_damage_instances_visited: u64,
    acc_damage_instances_matched: u64,
    acc_damage_commands_visited: u64,
    acc_damage_commands_matched: u64,
    acc_damage_vertices_visited: u64,
    acc_damage_query_ns: u64,
    acc_layer_body_commands_scanned: u64,
    acc_layer_body_commands_copied: u64,
    acc_layer_texture_creates: u32,
    acc_layer_cache_hits: u32,
    acc_layer_cache_misses: u32,
    acc_layer_offscreen_draws: u64,
    acc_layer_inline_draws: u64,
    acc_layer_double_render_prevented: u32,
    acc_id_mask_cache_hits: u32,
    acc_id_mask_cache_misses: u32,
    acc_id_mask_raster_passes: u32,
    acc_id_mask_field_seed_passes: u32,
    acc_id_mask_field_jump_passes: u32,
    acc_id_mask_compositor_passes: u32,
    acc_id_mask_target_creates: u32,
    acc_image_argument_encodes: u32,
    acc_image_argument_binds: u32,
    acc_image_argument_tables_finalized: u32,
    acc_image_argument_table_reuses: u32,
    acc_image_argument_bytes: u64,
    acc_image_argument_buffer_grows: u32,
    acc_render_passes: u32,
    acc_blit_passes: u32,
    acc_texture_copies: u32,
    acc_texture_copy_pixels: u64,
    acc_texture_copy_bytes: u64,
    acc_resource_creates: u32,
    acc_resource_grows: u32,
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
    submit_error_detail: Arc<Mutex<Option<String>>>,
    gpu_stage_timing: Option<GpuStageTimingSupport>,
    frame_gpu_trace: Option<GpuStageTrace>,
    completed_gpu_stats: Arc<Mutex<CompletedGpuStats>>,
    direct_preview_submitted: VecDeque<DirectPreviewSubmittedFrame>,
    direct_preview_last_submission_depth: u32,
    direct_preview_last_submission_skipped: u32,
    direct_preview_last_present_frame_age_ms: f64,
    direct_preview_last_completed_frame_id: u64,
    direct_preview_last_completed_gpu_ms: f64,
    direct_preview_last_completed_gpu_render_ms: f64,
    direct_preview_last_completed_gpu_vertex_ms: f64,
    direct_preview_last_completed_gpu_fragment_ms: f64,
    pending_present_drawable: usize,
    pending_present_texture: usize,
    frame_present_direct_to_drawable: bool,
    persistent_target_valid: bool,
    persistent_target_policy: u8,
    acc_damage_forced_full_refreshes: u32,
    frame_2d_encoded: bool,
    frame_color_initialized: bool,
    frame_depth_initialized: bool,
    frame_encode_started_at: Option<Instant>,
    accounting_stats_enabled: bool,
    memory_stats_enabled: bool,
    memory_texture_seen: RefCell<HashSet<usize>>,
    memory_buffer_seen: RefCell<HashSet<usize>>,
}

#[allow(dead_code)]
struct CameraPreviewRenderer {
    queue: CommandQueue,
    pso_camera: RenderPipelineState,
    pso_camera_legacy: RenderPipelineState,
    pso_camera_preview_fast_full: RenderPipelineState,
    pso_camera_preview_fast_video: RenderPipelineState,
    sampler: Option<SamplerState>,
    submit_error_flag: Arc<AtomicBool>,
    submit_error_detail: Arc<Mutex<Option<String>>>,
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
    command_buffer_ms: f64,
    encoder_ms: f64,
    setup_ms: f64,
    encode_quad_ms: f64,
    present_ms: f64,
    present_frame_age_ms: f64,
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
        sampler: Option<SamplerState>,
    ) -> Self {
        Self {
            queue,
            pso_camera,
            pso_camera_legacy,
            pso_camera_preview_fast_full,
            pso_camera_preview_fast_video,
            sampler,
            submit_error_flag: Arc::new(AtomicBool::new(false)),
            submit_error_detail: Arc::new(Mutex::new(None)),
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
        result.command_buffer_ms = command_buffer_ms;
        let rpd = RenderPassDescriptor::new();
        let setup_t0 = collect_stage_stats.then(Instant::now);
        let should_clear = direct_preview_should_clear_load_action(
            direct_preview_uses_dontcare_load_action(),
            true,
        );
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
            if should_clear {
                ca0.set_load_action(MTLLoadAction::Clear);
                ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
            } else {
                ca0.set_load_action(MTLLoadAction::DontCare);
            }
            Ok(())
        })?;
        result.setup_ms = elapsed_ms(setup_t0);

        let encoder_t0 = collect_stage_stats.then(Instant::now);
        let enc = cmd.new_render_command_encoder(&rpd);
        let encoder_ms = elapsed_ms(encoder_t0);
        result.encoder_ms = encoder_ms;
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
            if let Some(sam) = &self.sampler {
                enc.set_fragment_sampler_state(0, Some(sam));
            }
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
        result.present_frame_age_ms = direct_preview_present_frame_age_ms(frame.timestamp_ns);
        result.present_ms = elapsed_ms(present_t0);

        let submit_error_flag = Arc::clone(&self.submit_error_flag);
        let submit_error_detail = Arc::clone(&self.submit_error_detail);
        let inflight_submissions = Arc::clone(&self.inflight_submissions);
        let presented_generation = frame.generation;
        inflight_submissions.fetch_add(1, Ordering::AcqRel);
        let completion = ConcreteBlock::new(move |buffer: &CommandBufferRef| {
            if buffer.status() == MTLCommandBufferStatus::Error {
                let detail = unsafe { command_buffer_error_detail(buffer) };
                if let Ok(mut slot) = submit_error_detail.lock() {
                    *slot = detail.clone();
                }
                submit_error_flag.store(true, Ordering::Release);
            } else {
                mark_preview_generation_presented(presented_generation);
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
    #[inline]
    fn current_frame_slot(&self) -> usize {
        self.frame_slot
    }

    #[inline]
    fn next_frame_slot(&self) -> usize
    {
        let next = self.frame_slot + 1;
        if next == self.frames.len() { 0 } else { next }
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn mark_next_preferred_frame_slot_busy_for_snapshot(&mut self) -> usize
    {
        let preferred = self.next_frame_slot();
        self.frame_in_flight.fetch_or(frame_slot_bit(preferred), Ordering::Release);
        preferred
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn release_frame_slot_for_snapshot(&mut self, slot: usize)
    {
        if slot < self.frames.len()
        {
            self.frame_in_flight.fetch_and(!frame_slot_bit(slot), Ordering::Release);
        }
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn current_frame_slot_for_snapshot(&self) -> usize
    {
        self.current_frame_slot()
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn frame_resource_depth_for_snapshot(&self) -> usize
    {
        self.frames.len()
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn frame_ring_capacities_for_snapshot(&self, slot: usize) -> Option<[usize; 3]>
    {
        Some([*self.vb.cap.get(slot)?, *self.ib.cap.get(slot)?, *self.ub.cap.get(slot)?])
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn mark_frame_slot_busy_for_snapshot(&mut self, slot: usize)
    {
        if slot < self.frames.len()
        {
            self.frame_in_flight.fetch_or(frame_slot_bit(slot), Ordering::Release);
        }
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn current_frame_command_buffer_slot_for_snapshot(&self) -> Option<usize>
    {
        self.frames[self.current_frame_slot()]
            .cmd
            .as_ref()
            .map(|_| self.current_frame_slot())
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn frame_slot_has_command_buffer_for_snapshot(&self, slot: usize) -> bool
    {
        self.frames.get(slot).is_some_and(|frame| frame.cmd.is_some())
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn effect_target_presence_for_snapshot(&self) -> [bool; 8]
    {
        [
            self.prepass_tex.is_some(),
            self.half_tex.is_some(),
            self.quarter_tex.is_some(),
            self.quarter_tmp_tex.is_some(),
            self.eighth_tex.is_some(),
            self.eighth_tmp_tex.is_some(),
            self.scene3d_bloom_tex.is_some(),
            self.scene3d_bloom_tmp_tex.is_some(),
        ]
    }

    fn new_with_config_impl(config: MetalRendererConfig) -> Result<Self, MetalInitError> {
        let simulator = running_on_ios_simulator();
        let frame_resource_depth = config.frame_resource_depth.clamp(1, MAX_FRAME_RESOURCE_DEPTH);
        ios_log(&format!(
            "oxide.renderer-metal: init begin simulator={} wants_hdr={} sample_count={} camera_mode={:?} camera_source={:?} frame_resource_depth={}",
            simulator,
            config.wants_hdr,
            config.sample_count,
            config.camera_render_mode,
            config.camera_texture_source,
            frame_resource_depth
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
        if DEFAULT_METALLIB.is_empty() {
            return Err(MetalInitError::Library(String::from(
                "renderer-metal default.metallib is empty; build-time shader compilation is required",
            )));
        }
        ios_log("oxide.renderer-metal: init before shader library load");
        let library = device
            .new_library_with_data(DEFAULT_METALLIB)
            .map_err(|e| MetalInitError::Library(format!("{}", e)))?;
        ios_log("oxide.renderer-metal: init after shader library load");
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

        let direct_preview_only = config.direct_preview_only;
        let build_pipelines = |fmt: MTLPixelFormat| -> Result<_, MetalInitError> {
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
                build_image_pso(&device, &library, fmt, sample_count, false)
            })?;
            let pso_image_single = build_init_stage("pso.image_single", || {
                build_image_single_pso(&device, &library, fmt, sample_count, false)
            })?;
            let pso_image_mesh = build_init_stage("pso.image_mesh", || {
                build_image_mesh_pso(&device, &library, fmt, sample_count, false)
            })?;
            let pso_blur = build_init_stage("pso.blur", || {
                build_blur_pso(&device, &library, fmt, "f_blur", "pso.blur.create")
            })?;
            let pso_blur_paired = build_init_stage("pso.blur_paired", || {
                build_blur_pso(
                    &device,
                    &library,
                    fmt,
                    "f_blur_paired",
                    "pso.blur_paired.create",
                )
            })?;
            let pso_downsample = build_init_stage("pso.downsample", || {
                build_downsample_pso(&device, &library, fmt)
            })?;
            let pso_upsample =
                build_init_stage("pso.upsample", || build_upsample_pso(&device, &library, fmt))?;
            let pso_backdrop =
                build_init_stage("pso.backdrop", || build_backdrop_pso(&device, &library, fmt))?;
            let pso_visual_effect = build_init_stage("pso.visual_effect", || {
                build_visual_effect_pso(&device, &library, fmt)
            })?;
            let pso_rrect = build_init_stage("pso.rrect", || {
                build_rrect_pso(&device, &library, fmt, sample_count)
            })?;
            let pso_nine = build_init_stage("pso.nine_slice", || {
                build_nine_slice_pso(&device, &library, fmt, sample_count, false)
            })?;
            let pso_spin = build_init_stage("pso.spinner", || {
                build_spinner_pso(&device, &library, fmt, sample_count, false)
            })?;
            let pso_text = build_init_stage("pso.text", || {
                build_text_pso(&device, &library, fmt, sample_count, false)
            })?;
            let pso_text_sdf = build_init_stage("pso.text_sdf", || {
                build_text_sdf_pso(&device, &library, fmt, sample_count, false)
            })?;
            Ok((
                pso_solid,
                pso_image,
                pso_image_single,
                pso_image_mesh,
                pso_blur,
                pso_blur_paired,
                pso_downsample,
                pso_upsample,
                pso_backdrop,
                pso_visual_effect,
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
            pso_image_mesh,
            pso_blur,
            pso_blur_paired,
            pso_downsample,
            pso_upsample,
            pso_backdrop,
            pso_visual_effect,
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
            match build_pipelines(color_format) {
                Ok(pipelines) => break pipelines,
                Err(err) => {
                    if hdr_enabled {
                        hdr_enabled = false;
                        color_format = MTLPixelFormat::BGRA8Unorm_sRGB;
                        continue;
                    }
                    return Err(err);
                }
            }
        };
        let pso_layer_composite = if direct_preview_only {
            pso_nine.to_owned()
        } else {
            build_init_stage("pso.layer_composite", || {
                build_layer_composite_pso(&device, &library, color_format, sample_count)
            })?
        };
        let pso_layer_composite_aligned = if direct_preview_only {
            pso_nine.to_owned()
        } else {
            build_init_stage("pso.layer_composite_aligned", || {
                build_layer_composite_aligned_pso(&device, &library, color_format, sample_count)
            })?
        };
        let pso_layer_rrect = if direct_preview_only {
            pso_rrect.to_owned()
        } else {
            build_init_stage("pso.layer_rrect", || {
                build_layer_rrect_pso(&device, &library, color_format, sample_count)
            })?
        };
        let (
            pso_layer_image,
            pso_layer_image_single,
            pso_layer_image_mesh,
            pso_layer_nine_slice,
            pso_layer_spinner,
            pso_layer_text,
            pso_layer_text_sdf,
        ) = if direct_preview_only {
            (
                pso_image.to_owned(),
                pso_image_single.to_owned(),
                pso_image_mesh.to_owned(),
                pso_nine.to_owned(),
                pso_spin.to_owned(),
                pso_text.to_owned(),
                pso_text_sdf.to_owned(),
            )
        } else {
            (
                build_init_stage("pso.layer_image", || {
                    build_image_pso(&device, &library, color_format, sample_count, true)
                })?,
                build_init_stage("pso.layer_image_single", || {
                    build_image_single_pso(&device, &library, color_format, sample_count, true)
                })?,
                build_init_stage("pso.layer_image_mesh", || {
                    build_image_mesh_pso(&device, &library, color_format, sample_count, true)
                })?,
                build_init_stage("pso.layer_nine_slice", || {
                    build_nine_slice_pso(&device, &library, color_format, sample_count, true)
                })?,
                build_init_stage("pso.layer_spinner", || {
                    build_spinner_pso(&device, &library, color_format, sample_count, true)
                })?,
                build_init_stage("pso.layer_text", || {
                    build_text_pso(
                        &device,
                        &library,
                        color_format,
                        sample_count,
                        true,
                    )
                })?,
                build_init_stage("pso.layer_text_sdf", || {
                    build_text_sdf_pso(
                        &device,
                        &library,
                        color_format,
                        sample_count,
                        true,
                    )
                })?,
            )
        };
        let pso_scene3d_tri = build_init_stage("pso.scene3d.tri", || {
            build_scene3d_pso(
                &device,
                &library,
                color_format,
                false,
                scene3d::BlendMode3d::Alpha,
                scene3d::MeshTopology::Triangles,
                true,
            )
        })?;
        let pso_scene3d_tri_depth = build_init_stage("pso.scene3d.tri_depth", || {
            build_scene3d_pso(
                &device,
                &library,
                color_format,
                true,
                scene3d::BlendMode3d::Alpha,
                scene3d::MeshTopology::Triangles,
                true,
            )
        })?;
        let pso_scene3d_tri_add = build_init_stage("pso.scene3d.tri_add", || {
            build_scene3d_pso(
                &device,
                &library,
                color_format,
                false,
                scene3d::BlendMode3d::Additive,
                scene3d::MeshTopology::Triangles,
                true,
            )
        })?;
        let pso_scene3d_tri_add_bloom = build_init_stage("pso.scene3d.tri_add_bloom", || {
            build_scene3d_pso(
                &device,
                &library,
                MTLPixelFormat::RGBA16Float,
                false,
                scene3d::BlendMode3d::Additive,
                scene3d::MeshTopology::Triangles,
                false,
            )
        })?;
        let pso_scene3d_color_tri = build_init_stage("pso.scene3d.color_tri", || {
            build_scene3d_color_pso(
                &device,
                &library,
                color_format,
                scene3d::BlendMode3d::Alpha,
                true,
            )
        })?;
        let pso_scene3d_color_tri_add = build_init_stage("pso.scene3d.color_tri_add", || {
            build_scene3d_color_pso(
                &device,
                &library,
                color_format,
                scene3d::BlendMode3d::Additive,
                true,
            )
        })?;
        let pso_scene3d_color_tri_depth = build_init_stage("pso.scene3d.color_tri_depth", || {
            build_scene3d_color_pso(
                &device,
                &library,
                color_format,
                scene3d::BlendMode3d::Alpha,
                false,
            )
        })?;
        let pso_scene3d_line = build_init_stage("pso.scene3d.line", || {
            build_scene3d_pso(
                &device,
                &library,
                color_format,
                false,
                scene3d::BlendMode3d::Alpha,
                scene3d::MeshTopology::Lines,
                true,
            )
        })?;
        let pso_scene3d_line_depth = build_init_stage("pso.scene3d.line_depth", || {
            build_scene3d_pso(
                &device,
                &library,
                color_format,
                true,
                scene3d::BlendMode3d::Alpha,
                scene3d::MeshTopology::Lines,
                true,
            )
        })?;
        let pso_scene3d_line_add = build_init_stage("pso.scene3d.line_add", || {
            build_scene3d_pso(
                &device,
                &library,
                color_format,
                false,
                scene3d::BlendMode3d::Additive,
                scene3d::MeshTopology::Lines,
                true,
            )
        })?;
        let pso_scene3d_line_add_bloom = build_init_stage("pso.scene3d.line_add_bloom", || {
            build_scene3d_pso(
                &device,
                &library,
                MTLPixelFormat::RGBA16Float,
                false,
                scene3d::BlendMode3d::Additive,
                scene3d::MeshTopology::Lines,
                false,
            )
        })?;
        let pso_bloom_blur = build_init_stage("pso.bloom.blur", || {
            build_blur_pso(
                &device,
                &library,
                MTLPixelFormat::RGBA16Float,
                "f_blur",
                "pso.bloom.blur.create",
            )
        })?;
        let pso_bloom_blur_paired = if direct_preview_only {
            pso_bloom_blur.to_owned()
        } else {
            build_init_stage("pso.bloom.blur_paired", || {
                build_blur_pso(
                    &device,
                    &library,
                    MTLPixelFormat::RGBA16Float,
                    "f_blur_paired",
                    "pso.bloom.blur_paired.create",
                )
            })?
        };
        let pso_bloom_composite = build_init_stage("pso.bloom.composite", || {
            build_bloom_composite_pso(&device, &library, color_format)
        })?;
        let pso_id_mask_raster = build_init_stage("pso.id_mask_raster", || {
            id_mask_gpu::build_raster_pso(&device, &library)
        })?;
        let pso_id_mask_field_seed = build_init_stage("pso.id_mask_field_seed", || {
            id_mask_gpu::build_field_seed_pso(&device, &library)
        })?;
        let pso_id_mask_field_seed_wide = build_init_stage("pso.id_mask_field_seed_wide", || {
            id_mask_gpu::build_field_seed_wide_pso(&device, &library)
        })?;
        let pso_id_mask_field_jump = build_init_stage("pso.id_mask_field_jump", || {
            id_mask_gpu::build_field_jump_pso(&device, &library)
        })?;
        let pso_id_mask_field_jump_wide = build_init_stage("pso.id_mask_field_jump_wide", || {
            id_mask_gpu::build_field_jump_wide_pso(&device, &library)
        })?;
        let pso_id_mask_compositor = build_init_stage("pso.id_mask_compositor", || {
            id_mask_gpu::build_compositor_pso(&device, &library, color_format)
        })?;
        let pso_id_mask_compositor_wide = build_init_stage("pso.id_mask_compositor_wide", || {
            id_mask_gpu::build_compositor_wide_pso(&device, &library, color_format)
        })?;
        let pso_neon_marker = build_init_stage("pso.neon_marker", || {
            neon_marker_gpu::build_pso(&device, &library, color_format)
        })?;
        let prepared_pipelines = if direct_preview_only {
            None
        } else {
            Some(build_init_stage("pso.prepared", || {
                prepared::PreparedPipelines::new(
                    &device,
                    &library,
                    color_format,
                    sample_count,
                    false,
                )
            })?)
        };
        let prepared_layer_pipelines = if direct_preview_only {
            None
        } else {
            Some(build_init_stage("pso.prepared_layer", || {
                prepared::PreparedPipelines::new(
                    &device,
                    &library,
                    color_format,
                    1,
                    true,
                )
            })?)
        };
        let prepared_exact_layer_pipelines = if direct_preview_only {
            None
        } else {
            match build_init_stage("pso.prepared_exact_layer", || {
                prepared::PreparedPipelines::new(
                    &device,
                    &library,
                    MTLPixelFormat::RGBA32Float,
                    1,
                    true,
                )
            }) {
                Ok(pipelines) => Some(pipelines),
                Err(error) => {
                    eprintln!("[Oxide] renderer disabling exact prepared layers: {error}");
                    None
                }
            }
        };
        let depth_state_3d_disabled =
            build_depth_stencil_state(&device, false, false, "depth.scene3d.disabled");
        let depth_state_3d_read =
            build_depth_stencil_state(&device, true, false, "depth.scene3d.read");
        let depth_state_3d_write =
            build_depth_stencil_state(&device, true, true, "depth.scene3d.write");
        let depth_state_3d_write_no_test =
            build_depth_stencil_state(&device, false, true, "depth.scene3d.write_no_test");
        // Prepare argument encoder for image textures
        let (img_arg, img_arg_bufs, img_arg_stride) = if direct_preview_only {
            (None, None, 0)
        } else {
            let f_image_fn = pipeline_function(&library, "function.f_image", "f_image")?;
            let img_arg = Some(f_image_fn.new_argument_encoder(2));
            let encoder = img_arg.as_ref().unwrap();
            let img_arg_stride = align_up_usize(
                encoder.encoded_length() as usize,
                encoder.alignment().max(1) as usize,
            );
            let img_arg_bufs = Some(Ring::new(
                &device,
                frame_resource_depth,
                img_arg_stride.saturating_mul(IMAGE_ARG_SMALL_TABLE_COUNT),
                MTLResourceOptions::StorageModeShared,
            ));
            (img_arg, img_arg_bufs, img_arg_stride)
        };
        let sampler = build_sampler(&device);
        let opts =
            MTLResourceOptions::CPUCacheModeWriteCombined | MTLResourceOptions::StorageModeShared;
        let direct_preview_ring_size = 4 * 1024;
        // These capacities cover measured 4,096-quad and 1,024-marker high-water workloads.
        // Larger offscreen/stress frames retain bounded per-slot geometric growth.
        let vb = Ring::new(
            &device,
            frame_resource_depth,
            if direct_preview_only { direct_preview_ring_size } else { INITIAL_VERTEX_BUFFER_BYTES },
            opts,
        );
        let ib = Ring::new(
            &device,
            frame_resource_depth,
            if direct_preview_only { direct_preview_ring_size } else { INITIAL_INDEX_BUFFER_BYTES },
            opts,
        );
        let ub = Ring::new(
            &device,
            frame_resource_depth,
            if direct_preview_only { direct_preview_ring_size } else { INITIAL_UNIFORM_BUFFER_BYTES },
            opts,
        );
        let property_ring = Ring::new(
            &device,
            frame_resource_depth,
            if direct_preview_only { direct_preview_ring_size } else { INITIAL_PROPERTY_BUFFER_BYTES },
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
            frame_resource_depth,
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
                sampler.clone(),
            ))
        } else {
            None
        };
        let gpu_stage_timing = GpuStageTimingSupport::new(&device);
        let device_generation = NEXT_DEVICE_GENERATION.fetch_add(1, Ordering::Relaxed);
        let adaptive_layer_budget = device
            .recommended_max_working_set_size()
            .checked_div(16)
            .unwrap_or(0)
            .clamp(LAYER_CACHE_MIN_BUDGET_BYTES, LAYER_CACHE_MAX_BUDGET_BYTES);
        let layer_cache_budget_bytes = std::env::var("OXIDE_LAYER_CACHE_BUDGET_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(adaptive_layer_budget);
        let adaptive_id_mask_budget = device
            .recommended_max_working_set_size()
            .checked_div(8)
            .unwrap_or(0)
            .clamp(ID_MASK_CACHE_MIN_BUDGET_BYTES, ID_MASK_CACHE_MAX_BUDGET_BYTES);
        let id_mask_cache_budget_bytes = std::env::var("OXIDE_ID_MASK_CACHE_BUDGET_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(adaptive_id_mask_budget);

        Ok(Self {
            device,
            device_generation,
            queue,
            pso_solid,
            pso_image,
            pso_layer_image,
            pso_image_single,
            pso_layer_image_single,
            pso_image_mesh,
            pso_layer_image_mesh,
            pso_blur,
            pso_blur_paired,
            pso_downsample,
            pso_upsample,
            pso_backdrop,
            pso_visual_effect,
            pso_rrect,
            pso_layer_rrect,
            pso_nine_slice: pso_nine,
            pso_layer_nine_slice,
            pso_layer_composite,
            pso_layer_composite_aligned,
            pso_spinner: pso_spin,
            pso_layer_spinner,
            pso_text,
            pso_layer_text,
            pso_text_sdf,
            pso_layer_text_sdf,
            prepared_pipelines,
            pso_camera,
            pso_camera_legacy,
            pso_camera_preview_fast_full,
            pso_camera_preview_fast_video,
            pso_camera_bgra,
            pso_scene3d_tri,
            pso_scene3d_tri_depth,
            pso_scene3d_tri_add,
            pso_scene3d_tri_add_bloom,
            pso_scene3d_color_tri,
            pso_scene3d_color_tri_add,
            pso_scene3d_color_tri_depth,
            pso_scene3d_line,
            pso_scene3d_line_depth,
            pso_scene3d_line_add,
            pso_scene3d_line_add_bloom,
            pso_bloom_blur,
            pso_bloom_blur_paired,
            pso_bloom_composite,
            pso_id_mask_raster,
            pso_id_mask_field_seed,
            pso_id_mask_field_seed_wide,
            pso_id_mask_field_jump,
            pso_id_mask_field_jump_wide,
            pso_id_mask_compositor,
            pso_id_mask_compositor_wide,
            pso_neon_marker,
            depth_state_3d_disabled,
            depth_state_3d_read,
            depth_state_3d_write,
            depth_state_3d_write_no_test,
            prepared_layer_pipelines,
            prepared_exact_layer_pipelines,
            img_arg,
            img_arg_bufs,
            img_arg_stride,
            img_arg_used: 0,
            image_arg_tables: alloc::vec::Vec::new(),
            image_arg_table_index: HashMap::new(),
            image_arg_table_count: 0,
            image_arg_handles: alloc::vec::Vec::new(),
            image_vbuf: alloc::vec::Vec::new(),
            image_fbuf: alloc::vec::Vec::new(),
            effect_fbuf: alloc::vec::Vec::new(),
            effect_graph_events: alloc::vec::Vec::new(),
            effect_graph_plan: api::EffectGraphPlan::default(),
            effect_graph_key: 0,
            #[cfg(feature = "snapshot-tests")]
            force_exact_blur_for_snapshot: false,
            sampler,
            color_format,
            config: applied_config,
            sample_count,
            hdr_enabled,
            frame_id: 0,
            frame_slot: 0,
            frame_backpressure_skipped: false,
            frame_in_flight: Arc::new(AtomicU8::new(0)),
            frames: (0..frame_resource_depth).map(|_| PerFrame::new()).collect(),
            vb,
            ib,
            ub,
            property_ring,
            image_tex_map: HashMap::new(),
            glyph_instances: alloc::vec::Vec::new(),
            filtered_prepass: FilteredDrawList::default(),
            filtered_main: FilteredDrawList::default(),
            layer_plans: alloc::vec::Vec::new(),
            layer_plan_stack: alloc::vec::Vec::new(),
            layer_sublist: api::DrawList::default(),
            layer_scratch_frame: None,
            clip_stack_pool: alloc::vec::Vec::new(),
            target_w: 0,
            target_h: 0,
            target_scale: 1.0,
            target_tex: None,
            target_msaa_tex: None,
            depth_tex: None,
            prepass_tex: None,
            half_tex: None,
            quarter_tex: None,
            quarter_tmp_tex: None,
            eighth_tex: None,
            eighth_tmp_tex: None,
            scene3d_bloom_tex: None,
            scene3d_bloom_tmp_tex: None,
            id_mask_snapshot_target: None,
            id_mask_in_flight_generations: (0..frame_resource_depth)
                .map(|_| alloc::vec::Vec::new())
                .collect(),
            id_mask_field_cache: alloc::vec::Vec::new(),
            id_mask_frame_cache_serials: alloc::vec::Vec::new(),
            id_mask_cache_budget_bytes,
            id_mask_cache_resident_bytes: 0,
            id_mask_cache_evictions: 0,
            next_id_mask_cache_serial: 1,
            id_mask_target_peak_bytes: 0,
            id_mask_target_reuse_blocked: 0,
            id_mask_vertex_caches: alloc::vec::Vec::new(),
            images: HashMap::new(),
            image_generations: HashMap::new(),
            next_image_id: 1,
            prepared_chunks: prepared::PreparedChunkCache::default(),
            prepared_property_cache: prepared::PreparedPropertyCache::default(),
            prepared_frame_plan: alloc::vec::Vec::new(),
            prepared_frame_snapshot: None,
            prepared_frame_viewport: [0.0, 0.0],
            prepared_frame_keys: alloc::vec::Vec::new(),
            prepared_layer_frame_keys: HashMap::new(),
            prepared_damage_instances: alloc::vec::Vec::new(),
            prepared_damage_commands: alloc::vec::Vec::new(),
            prepared_fallback: api::DrawList::default(),
            meshes_3d: HashMap::new(),
            next_mesh3d_id: 1,
            scene3d_draws: alloc::vec::Vec::new(),
            layers: HashMap::new(),
            layer_pool: alloc::vec::Vec::new(),
            layer_frame_ids: HashSet::new(),
            layer_cache_budget_bytes,
            layer_cache_resident_bytes: 0,
            layer_cache_pool_bytes: 0,
            layer_cache_pool_reuses: 0,
            layer_cache_evictions: 0,
            layer_cache_recreations: 0,
            layer_cache_purges: 0,
            layer_cache_last_purge_reason: LAYER_PURGE_NONE,
            layer_cache_enabled,
            encoding_layer: false,
            inline_layer_counter_active: false,
            last_stats: PerfStats::default(),
            acc_draws: 0,
            acc_flat_instanced_draws: 0,
            acc_instanced: 0,
            acc_analytic_instance_bytes: 0,
            acc_analytic_instance_buffer_binds: 0,
            acc_analytic_instance_ring_grows: 0,
            acc_scene3d_draws: 0,
            acc_scene3d_instances: 0,
            acc_scene3d_instance_bytes: 0,
            acc_scene3d_pipeline_binds: 0,
            acc_scene3d_depth_state_binds: 0,
            acc_scene3d_cull_sets: 0,
            acc_scene3d_mesh_buffer_binds: 0,
            acc_scene3d_instance_buffer_binds: 0,
            acc_scene3d_instance_ring_grows: 0,
            acc_scene3d_viewport_sets: 0,
            acc_glyph_instance_bytes: 0,
            acc_glyph_instance_buffer_binds: 0,
            acc_glyph_instances: 0,
            acc_icb_cmds: 0,
            acc_commands_traversed: 0,
            acc_commands_copied: 0,
            acc_geometry_bytes_copied: 0,
            acc_chunks_reused: 0,
            acc_chunks_rebuilt: 0,
            acc_chunks_prepared: 0,
            acc_prepared_plan_reuses: 0,
            acc_backend_cache_hits: 0,
            acc_backend_cache_misses: 0,
            acc_damage_instances_visited: 0,
            acc_damage_instances_matched: 0,
            acc_damage_commands_visited: 0,
            acc_damage_commands_matched: 0,
            acc_damage_vertices_visited: 0,
            acc_damage_query_ns: 0,
            acc_layer_body_commands_scanned: 0,
            acc_layer_body_commands_copied: 0,
            acc_layer_texture_creates: 0,
            acc_layer_cache_hits: 0,
            acc_layer_cache_misses: 0,
            acc_layer_offscreen_draws: 0,
            acc_layer_inline_draws: 0,
            acc_layer_double_render_prevented: 0,
            acc_id_mask_cache_hits: 0,
            acc_id_mask_cache_misses: 0,
            acc_id_mask_raster_passes: 0,
            acc_id_mask_field_seed_passes: 0,
            acc_id_mask_field_jump_passes: 0,
            acc_id_mask_compositor_passes: 0,
            acc_id_mask_target_creates: 0,
            acc_image_argument_encodes: 0,
            acc_image_argument_binds: 0,
            acc_image_argument_tables_finalized: 0,
            acc_image_argument_table_reuses: 0,
            acc_image_argument_bytes: 0,
            acc_image_argument_buffer_grows: 0,
            acc_render_passes: 0,
            acc_blit_passes: 0,
            acc_texture_copies: 0,
            acc_texture_copy_pixels: 0,
            acc_texture_copy_bytes: 0,
            acc_resource_creates: 0,
            acc_resource_grows: 0,
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
            submit_error_detail: Arc::new(Mutex::new(None)),
            gpu_stage_timing,
            frame_gpu_trace: None,
            completed_gpu_stats: Arc::new(Mutex::new(CompletedGpuStats::default())),
            direct_preview_submitted: VecDeque::new(),
            direct_preview_last_submission_depth: 0,
            direct_preview_last_submission_skipped: 0,
            direct_preview_last_present_frame_age_ms: 0.0,
            direct_preview_last_completed_frame_id: 0,
            direct_preview_last_completed_gpu_ms: 0.0,
            direct_preview_last_completed_gpu_render_ms: 0.0,
            direct_preview_last_completed_gpu_vertex_ms: 0.0,
            direct_preview_last_completed_gpu_fragment_ms: 0.0,
            pending_present_drawable: 0,
            pending_present_texture: 0,
            frame_present_direct_to_drawable: false,
            persistent_target_valid: false,
            persistent_target_policy: 0,
            acc_damage_forced_full_refreshes: 0,
            frame_2d_encoded: false,
            frame_color_initialized: false,
            frame_depth_initialized: false,
            frame_encode_started_at: None,
            accounting_stats_enabled: true,
            memory_stats_enabled: true,
            memory_texture_seen: RefCell::new(HashSet::with_capacity(64)),
            memory_buffer_seen: RefCell::new(HashSet::with_capacity(32)),
        })
    }

    pub fn new_with_config(config: MetalRendererConfig) -> Result<Self, MetalInitError> {
        Self::new_with_config_impl(config)
    }

    pub fn new_default() -> Result<Self, MetalInitError> {
        Self::new_with_config(MetalRendererConfig::default())
    }

    pub fn set_camera_render_mode(&mut self, mode: CameraRenderMode) {
        if self.camera_render_mode != mode {
            self.persistent_target_valid = false;
        }
        self.camera_render_mode = mode;
        self.config.camera_render_mode = mode;
    }

    pub fn set_camera_texture_source(&mut self, source: CameraTextureSource) {
        if self.camera_texture_source != source {
            self.release_live_camera_frame();
            self.persistent_target_valid = false;
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
        while let Some(submission) = self.direct_preview_submitted.front().cloned() {
            let status = submission.cmd.status();
            match status {
                MTLCommandBufferStatus::Completed => {
                    self.direct_preview_last_completed_frame_id = submission.frame_id;
                    mark_preview_generation_presented(submission.generation);
                    self.direct_preview_last_completed_gpu_ms =
                        unsafe { command_buffer_gpu_duration_ms(&submission.cmd) };
                    let gpu_stage_stats = submission
                        .gpu_trace
                        .as_ref()
                        .map(|trace| trace.resolve(&self.device))
                        .unwrap_or_default();
                    self.direct_preview_last_completed_gpu_render_ms = gpu_stage_stats.render_ms;
                    self.direct_preview_last_completed_gpu_vertex_ms = gpu_stage_stats.vertex_ms;
                    self.direct_preview_last_completed_gpu_fragment_ms =
                        gpu_stage_stats.fragment_ms;
                    self.direct_preview_submitted.pop_front();
                }
                MTLCommandBufferStatus::Error => {
                    if log_enabled {
                        unsafe {
                            let err: *mut Object = msg_send![submission.cmd.as_ptr(), error];
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
                                    submission.frame_id, domain, code, desc
                                ));
                            } else {
                                ios_log(&format!(
                                    "oxide.renderer-metal: direct preview submit error frame={} error=nil",
                                    submission.frame_id
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

    fn track_direct_preview_submission(
        &mut self,
        frame_id: u64,
        generation: u64,
        cmd: &CommandBuffer,
        gpu_trace: Option<GpuStageTrace>,
    ) {
        self.direct_preview_submitted.push_back(DirectPreviewSubmittedFrame {
            frame_id,
            generation,
            cmd: cmd.to_owned(),
            gpu_trace,
        });
    }

    #[inline]
    fn latest_completed_gpu_stats(&self) -> CompletedGpuStats {
        self.completed_gpu_stats.lock().map(|stats| *stats).unwrap_or_default()
    }

    #[inline]
    fn apply_completed_gpu_stats(&self, stats: &mut PerfStats) {
        let gpu = self.latest_completed_gpu_stats();
        stats.gpu_frame_id = gpu.frame_id;
        stats.gpu_ms = gpu.command_ms;
        stats.gpu_render_ms = gpu.render_ms;
        stats.gpu_vertex_ms = gpu.vertex_ms;
        stats.gpu_fragment_ms = gpu.fragment_ms;
    }

    #[inline]
    fn note_direct_preview_submission_depth(&mut self) -> u32 {
        let depth = self.current_preview_submission_depth() as u32;
        self.direct_preview_last_submission_depth = depth;
        depth
    }

    #[inline]
    fn current_preview_submission_depth(&self) -> usize {
        self.camera_preview_renderer
            .as_ref()
            .map(|renderer| renderer.pending_submission_count() as usize)
            .unwrap_or(self.direct_preview_submitted.len())
    }

    #[inline]
    fn direct_preview_backpressure_blocks_present(&mut self) -> bool {
        let depth = self.note_direct_preview_submission_depth() as usize;
        let blocked = direct_preview_submission_backpressure_applies(
            experimental_preview_submission_cap(),
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
            self.persistent_target_valid = false;
            self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
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
                self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
            }
        } else {
            self.target_msaa_tex = None;
        }
    }

    fn ensure_depth_target(&mut self) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        let need_new = match &self.depth_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        if !need_new {
            return;
        }

        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::Depth32Float);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(self.target_w as u64);
        desc.set_height(self.target_h as u64);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget);
        self.depth_tex = Some(self.device.new_texture(&desc));
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
    }

    fn ensure_frame_command_buffer(&mut self, slot: usize) -> CommandBuffer {
        if let Some(cmd) = self.frames[slot].cmd.as_ref() {
            return cmd.to_owned();
        }
        let cmd = self.queue.new_command_buffer().to_owned();
        self.frames[slot].cmd = Some(cmd.to_owned());
        cmd
    }

    fn drop_direct_preview_offscreen_targets(&mut self) {
        self.persistent_target_valid = false;
        self.target_tex = None;
        self.target_msaa_tex = None;
        self.depth_tex = None;
        self.purge_effect_targets();
        self.id_mask_snapshot_target = None;
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
        let target_size_changed = self.target_w != target_w || self.target_h != target_h;
        self.target_w = target_w;
        self.target_h = target_h;
        self.target_scale = target_scale;
        if self.sample_count == 1 {
            self.drop_direct_preview_offscreen_targets();
        } else if target_size_changed {
            self.purge_effect_targets();
        }
    }

    fn ensure_effect_targets(&mut self, plan: EffectTargetPlan) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        match plan {
            EffectTargetPlan::Prepass => {
                self.half_tex = None;
                self.quarter_tex = None;
                self.quarter_tmp_tex = None;
                self.eighth_tex = None;
                self.eighth_tmp_tex = None;
            }
            EffectTargetPlan::Quarter => {
                self.eighth_tex = None;
                self.eighth_tmp_tex = None;
            }
            EffectTargetPlan::Eighth => {
                self.quarter_tmp_tex = None;
            }
        }
        let full = (self.target_w as u64, self.target_h as u64);
        let half = (((self.target_w / 2).max(1)) as u64, ((self.target_h / 2).max(1)) as u64);
        let quarter = (((self.target_w / 4).max(1)) as u64, ((self.target_h / 4).max(1)) as u64);
        let eighth = (((self.target_w / 8).max(1)) as u64, ((self.target_h / 8).max(1)) as u64);
        let device = &self.device;
        let format = self.color_format;
        let creates = &mut self.acc_resource_creates;
        ensure_effect_texture(device, format, &mut self.prepass_tex, full, creates);
        if plan.uses_blur_chain() {
            ensure_effect_texture(device, format, &mut self.half_tex, half, creates);
            ensure_effect_texture(device, format, &mut self.quarter_tex, quarter, creates);
        }
        if plan == EffectTargetPlan::Quarter {
            ensure_effect_texture(device, format, &mut self.quarter_tmp_tex, quarter, creates);
        }
        if plan == EffectTargetPlan::Eighth {
            ensure_effect_texture(device, format, &mut self.eighth_tex, eighth, creates);
            ensure_effect_texture(device, format, &mut self.eighth_tmp_tex, eighth, creates);
        }
    }

    /// Releases retained effect and bloom render targets after memory pressure.
    pub fn purge_effect_targets(&mut self) {
        self.prepass_tex = None;
        self.half_tex = None;
        self.quarter_tex = None;
        self.quarter_tmp_tex = None;
        self.eighth_tex = None;
        self.eighth_tmp_tex = None;
        self.scene3d_bloom_tex = None;
        self.scene3d_bloom_tmp_tex = None;
    }

   /// Returns the hard allocated-byte budget for persistent prepared chunks.
   pub fn prepared_cache_budget_bytes(&self) -> u64
   {
      self.prepared_chunks.budget_bytes()
   }

   /// Applies a new hard prepared-chunk budget and evicts cold entries immediately.
   pub fn set_prepared_cache_budget_bytes(&mut self, budget_bytes: u64)
   {
      self.prepared_chunks.set_budget_bytes(budget_bytes);
   }

   /// Returns Metal-allocated bytes currently retained by prepared chunk buffers.
   pub fn prepared_cache_resident_bytes(&self) -> u64
   {
      self.prepared_chunks.resident_bytes()
   }

   /// Returns the number of chunk versions admitted to the prepared cache.
   pub fn prepared_cache_entry_count(&self) -> usize
   {
      self.prepared_chunks.len()
   }

   /// Releases every prepared chunk, including resource tables and immutable geometry.
   pub fn purge_prepared_chunks(&mut self)
   {
      self.prepared_chunks.clear();
      for layer in self.layers.values_mut()
      {
         layer.prepared_key = None;
         layer.resources.clear();
      }
   }

   /// Returns the hard Metal-allocated-byte budget for retained and pooled layers.
   pub fn layer_cache_budget_bytes(&self) -> u64
   {
      self.layer_cache_budget_bytes
   }

   /// Applies a hard retained-layer budget and immediately releases cold storage.
   pub fn set_layer_cache_budget_bytes(&mut self, budget_bytes: u64)
   {
      self.layer_cache_budget_bytes = budget_bytes;
      self.enforce_layer_cache_budget();
      self.apply_layer_cache_stats();
   }

   /// Releases every retained and pooled layer texture.
   pub fn purge_layer_cache(&mut self)
   {
      self.purge_layer_cache_for_reason(LAYER_PURGE_EXPLICIT);
   }

   /// Releases every layer texture in response to an operating-system memory warning.
   pub fn purge_layer_cache_for_memory_warning(&mut self)
   {
      self.purge_layer_cache_for_reason(LAYER_PURGE_MEMORY_WARNING);
   }

   /// Returns the hard byte budget for immutable ID-mask raster and JFA fields.
   pub fn id_mask_cache_budget_bytes(&self) -> u64
   {
      self.id_mask_cache_budget_bytes
   }

   /// Applies a hard ID-mask field-cache budget and immediately evicts cold entries.
   pub fn set_id_mask_cache_budget_bytes(&mut self, budget_bytes: u64)
   {
      self.id_mask_cache_budget_bytes = budget_bytes;
      self.enforce_id_mask_cache_budget();
      self.apply_id_mask_cache_stats();
   }

   /// Releases all immutable ID-mask raster and JFA field entries.
   pub fn purge_id_mask_field_cache(&mut self)
   {
      self.id_mask_field_cache.clear();
      self.id_mask_frame_cache_serials.clear();
      self.id_mask_snapshot_target = None;
      let busy_slots = self.frame_in_flight.load(Ordering::Acquire);
      self.clear_completed_id_mask_generations(busy_slots);
      self.id_mask_cache_resident_bytes = 0;
      self.apply_id_mask_cache_stats();
   }

   #[inline]
   fn next_id_mask_generation_serial(&mut self) -> u64
   {
      let serial = self.next_id_mask_cache_serial;
      self.next_id_mask_cache_serial = serial.wrapping_add(1).max(1);
      serial
   }

   fn clear_completed_id_mask_generations(&mut self, busy_slots: u8)
   {
      for (slot, generations) in self.id_mask_in_flight_generations.iter_mut().enumerate()
      {
         if busy_slots & frame_slot_bit(slot) == 0
         {
            generations.clear();
         }
      }
   }

   fn retain_id_mask_in_flight_generation(&mut self, serial: u64, bytes: u64)
   {
      let slot = self.current_frame_slot();
      let Some(generations) = self.id_mask_in_flight_generations.get_mut(slot) else
      {
         return;
      };
      if !generations.iter().any(|generation| generation.serial == serial)
      {
         generations.push(id_mask_gpu::IdMaskInFlightGeneration { serial, bytes });
      }
      self.update_id_mask_target_peak();
   }

   fn id_mask_generation_in_flight(&self, serial: u64) -> bool
   {
      let busy_slots = self.frame_in_flight.load(Ordering::Acquire);
      self.id_mask_in_flight_generations.iter().enumerate().any(|(slot, generations)| {
         busy_slots & frame_slot_bit(slot) != 0
            && generations.iter().any(|generation| generation.serial == serial)
      })
   }

   #[inline]
   fn id_mask_generation_slot_active(&self, slot: usize, busy_slots: u8) -> bool
   {
      busy_slots & frame_slot_bit(slot) != 0
         || (slot == self.current_frame_slot()
            && self.frames.get(slot).is_some_and(|frame| frame.cmd.is_some()))
   }

   fn id_mask_target_storage(&self) -> (u32, u64, u64)
   {
      let busy_slots = self.frame_in_flight.load(Ordering::Acquire);
      let mut count = 0_u32;
      let mut in_flight_bytes = 0_u64;
      let mut storage_bytes = self.id_mask_cache_resident_bytes;
      for (slot, generations) in self.id_mask_in_flight_generations.iter().enumerate()
      {
         if !self.id_mask_generation_slot_active(slot, busy_slots)
         {
            continue;
         }
         for (index, generation) in generations.iter().enumerate()
         {
            let seen_in_slot = generations[..index].iter()
               .any(|seen| seen.serial == generation.serial);
            let seen_in_prior_slot = self.id_mask_in_flight_generations[..slot]
               .iter()
               .enumerate()
               .any(|(prior_slot, prior)| {
                  self.id_mask_generation_slot_active(prior_slot, busy_slots)
                     && prior.iter().any(|seen| seen.serial == generation.serial)
               });
            if !seen_in_slot && !seen_in_prior_slot
            {
               count = count.saturating_add(1);
               in_flight_bytes = in_flight_bytes.saturating_add(generation.bytes);
               if !self.id_mask_field_cache.iter()
                  .any(|entry| entry.serial == generation.serial)
               {
                  storage_bytes = storage_bytes.saturating_add(generation.bytes);
               }
            }
         }
      }
      (count, in_flight_bytes, storage_bytes)
   }

   fn update_id_mask_target_peak(&mut self)
   {
      self.id_mask_target_peak_bytes = self.id_mask_target_peak_bytes
         .max(self.id_mask_target_storage().2);
   }

   fn apply_id_mask_cache_stats(&mut self)
   {
      let (in_flight_generations, in_flight_bytes, target_storage_bytes) =
         self.id_mask_target_storage();
      self.id_mask_target_peak_bytes = self.id_mask_target_peak_bytes.max(target_storage_bytes);
      self.last_stats.id_mask_cache_hits = self.acc_id_mask_cache_hits;
      self.last_stats.id_mask_cache_misses = self.acc_id_mask_cache_misses;
      self.last_stats.id_mask_cache_budget_bytes = self.id_mask_cache_budget_bytes;
      self.last_stats.id_mask_cache_resident_bytes = self.id_mask_cache_resident_bytes;
      self.last_stats.id_mask_cache_evictions = self.id_mask_cache_evictions;
      self.last_stats.id_mask_cache_entries = self.id_mask_field_cache.len() as u32;
      self.last_stats.id_mask_raster_passes = self.acc_id_mask_raster_passes;
      self.last_stats.id_mask_field_seed_passes = self.acc_id_mask_field_seed_passes;
      self.last_stats.id_mask_field_jump_passes = self.acc_id_mask_field_jump_passes;
      self.last_stats.id_mask_compositor_passes = self.acc_id_mask_compositor_passes;
      self.last_stats.id_mask_target_creates = self.acc_id_mask_target_creates;
      self.last_stats.id_mask_in_flight_generations = in_flight_generations;
      self.last_stats.id_mask_in_flight_target_bytes = in_flight_bytes;
      self.last_stats.id_mask_target_storage_bytes = target_storage_bytes;
      self.last_stats.id_mask_target_peak_bytes = self.id_mask_target_peak_bytes;
      self.last_stats.id_mask_target_reuse_blocked = self.id_mask_target_reuse_blocked;
   }

   fn evict_oldest_id_mask_cache_entry(&mut self) -> Option<(id_mask_gpu::RenderTargets, bool)>
   {
      let index = self.id_mask_field_cache.iter().enumerate()
         .filter(|(_, entry)| !self.id_mask_frame_cache_serials.contains(&entry.serial))
         .min_by_key(|(_, entry)| entry.last_used_frame)
         .map(|(index, _)| index)?;
      let entry = self.id_mask_field_cache.swap_remove(index);
      self.id_mask_cache_resident_bytes = self.id_mask_cache_resident_bytes
         .saturating_sub(entry.bytes);
      self.id_mask_cache_evictions = self.id_mask_cache_evictions.saturating_add(1);
      let reusable = !self.id_mask_generation_in_flight(entry.serial);
      Some((entry.targets, reusable))
   }

   fn enforce_id_mask_cache_budget(&mut self)
   {
      while self.id_mask_field_cache.len() > ID_MASK_CACHE_MAX_ENTRIES
         || self.id_mask_cache_resident_bytes > self.id_mask_cache_budget_bytes
      {
         if self.evict_oldest_id_mask_cache_entry().is_none()
         {
            break;
         }
      }
   }

   fn prepare_id_mask_cache_admission(&mut self, required: u64, width: usize, height: usize) -> Option<Option<id_mask_gpu::RenderTargets>>
   {
      if required > self.id_mask_cache_budget_bytes
      {
         while !self.id_mask_field_cache.is_empty()
         {
            if self.evict_oldest_id_mask_cache_entry().is_none()
            {
               break;
            }
         }
         return None;
      }
      let mut reusable = None;
      while self.id_mask_field_cache.len() >= ID_MASK_CACHE_MAX_ENTRIES
         || self.id_mask_cache_resident_bytes.saturating_add(required)
            > self.id_mask_cache_budget_bytes
      {
         let (targets, synchronization_permits_reuse) =
            self.evict_oldest_id_mask_cache_entry()?;
         if targets.width == width && targets.height == height
         {
            if reusable.is_none() && synchronization_permits_reuse
            {
               reusable = Some(targets);
            }
            else if !synchronization_permits_reuse
            {
               self.id_mask_target_reuse_blocked = self.id_mask_target_reuse_blocked
                  .saturating_add(1);
            }
         }
      }
      Some(reusable)
   }

   fn layer_texture_descriptor(format: MTLPixelFormat, width: u32, height: u32) -> TextureDescriptor
   {
      let descriptor = TextureDescriptor::new();
      descriptor.set_pixel_format(format);
      descriptor.set_texture_type(MTLTextureType::D2);
      descriptor.set_width(width as u64);
      descriptor.set_height(height as u64);
      descriptor.set_storage_mode(MTLStorageMode::Private);
      descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
      descriptor
   }

   fn layer_texture_required_bytes(&self, format: MTLPixelFormat, width: u32, height: u32) -> u64
   {
      let descriptor = Self::layer_texture_descriptor(format, width, height);
      self.device.heap_texture_size_and_align(&descriptor).size as u64
   }

   fn layer_cache_cpu_bytes(&self) -> u64
   {
      let active = (self.layers.len() as u64)
         .saturating_mul(core::mem::size_of::<LayerEntry>() as u64);
      let pooled = (self.layer_pool.capacity() as u64)
         .saturating_mul(core::mem::size_of::<LayerPoolEntry>() as u64);
      let resources = self.layers.values().fold(0_u64, |total, layer| {
         total.saturating_add(
            (layer.resources.capacity() as u64)
               .saturating_mul(core::mem::size_of::<api::RenderResourceDependency>() as u64),
         )
      });
      active.saturating_add(pooled).saturating_add(resources)
   }

   fn apply_layer_cache_stats(&mut self)
   {
      self.last_stats.layer_cache_budget_bytes = self.layer_cache_budget_bytes;
      self.last_stats.layer_cache_resident_bytes = self.layer_cache_resident_bytes;
      self.last_stats.layer_cache_pool_bytes = self.layer_cache_pool_bytes;
      self.last_stats.layer_cache_cpu_bytes = self.layer_cache_cpu_bytes();
      self.last_stats.layer_cache_oldest_last_used_frame = self.layers
         .values()
         .map(|layer| layer.last_used_frame)
         .min()
         .unwrap_or(0);
      self.last_stats.layer_cache_pool_reuses = self.layer_cache_pool_reuses;
      self.last_stats.layer_cache_evictions = self.layer_cache_evictions;
      self.last_stats.layer_cache_recreations = self.layer_cache_recreations;
      self.last_stats.layer_cache_purges = self.layer_cache_purges;
      self.last_stats.layer_cache_last_purge_reason = self.layer_cache_last_purge_reason;
   }

   fn purge_layer_cache_for_reason(&mut self, reason: u8)
   {
      let removed = self.layers.len().saturating_add(self.layer_pool.len());
      self.layers.clear();
      self.layer_pool.clear();
      self.layer_frame_ids.clear();
      self.layer_cache_resident_bytes = 0;
      self.layer_cache_pool_bytes = 0;
      self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(removed as u64);
      self.layer_cache_purges = self.layer_cache_purges.saturating_add(1);
      self.layer_cache_last_purge_reason = reason;
      self.prepared_frame_snapshot = None;
      self.prepared_frame_plan.clear();
      self.prepared_layer_frame_keys.clear();
      self.apply_layer_cache_stats();
   }

   fn trim_layer_pool(&mut self, max_bytes: u64)
   {
      while self.layer_cache_pool_bytes > max_bytes
      {
         let Some(index) = self.layer_pool.iter().enumerate()
            .min_by_key(|(_, entry)| entry.recycled_frame)
            .map(|(index, _)| index) else { break };
         let removed = self.layer_pool.swap_remove(index);
         self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_sub(removed.bytes);
         self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
      }
   }

   fn recycle_layer_entry(&mut self, entry: LayerEntry)
   {
      self.layer_cache_resident_bytes = self.layer_cache_resident_bytes.saturating_sub(entry.bytes);
      let pool_budget = self.layer_cache_budget_bytes
         .checked_div(LAYER_CACHE_POOL_BUDGET_DIVISOR)
         .unwrap_or(0);
      if entry.bytes > pool_budget
      {
         self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
         return;
      }
      self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_add(entry.bytes);
      self.layer_pool.push(LayerPoolEntry {
         format: entry.tex.pixel_format(),
         tex: entry.tex,
         w: entry.w,
         h: entry.h,
         bytes: entry.bytes,
         recycled_frame: self.frame_id,
      });
      self.trim_layer_pool(pool_budget);
   }

   fn evict_oldest_unprotected_layer(&mut self) -> bool
   {
      let Some(id) = self.layers.iter()
         .filter(|(id, _)| !self.layer_frame_ids.contains(id))
         .min_by_key(|(_, layer)| layer.last_used_frame)
         .map(|(id, _)| *id) else { return false };
      if let Some(layer) = self.layers.remove(&id)
      {
         self.recycle_layer_entry(layer);
         self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
      }
      true
   }

   fn enforce_layer_cache_budget(&mut self)
   {
      self.trim_layer_pool(self.layer_cache_budget_bytes.saturating_sub(self.layer_cache_resident_bytes));
      while self.layer_cache_resident_bytes.saturating_add(self.layer_cache_pool_bytes)
         > self.layer_cache_budget_bytes
      {
         if !self.evict_oldest_unprotected_layer()
         {
            break;
         }
         self.trim_layer_pool(self.layer_cache_budget_bytes.saturating_sub(self.layer_cache_resident_bytes));
      }
   }

   fn admit_layer_bytes(&mut self, bytes: u64) -> bool
   {
      if bytes > self.layer_cache_budget_bytes
      {
         return false;
      }
      let retained_limit = self.layer_cache_budget_bytes.saturating_sub(bytes);
      self.trim_layer_pool(retained_limit.saturating_sub(self.layer_cache_resident_bytes));
      while self.layer_cache_resident_bytes.saturating_add(self.layer_cache_pool_bytes) > retained_limit
      {
         if !self.evict_oldest_unprotected_layer()
         {
            return false;
         }
         self.trim_layer_pool(retained_limit.saturating_sub(self.layer_cache_resident_bytes));
      }
      true
   }

   fn take_pooled_layer_texture(&mut self, format: MTLPixelFormat, width: u32, height: u32) -> Option<(Texture, u64)>
   {
      let index = self.layer_pool.iter().position(|entry| {
         entry.format == format && entry.w == width && entry.h == height
      })?;
      let entry = self.layer_pool.swap_remove(index);
      self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_sub(entry.bytes);
      self.layer_cache_resident_bytes = self.layer_cache_resident_bytes.saturating_add(entry.bytes);
      self.layer_cache_pool_reuses = self.layer_cache_pool_reuses.saturating_add(1);
      Some((entry.tex, entry.bytes))
   }

   fn recycle_compatible_unprotected_layer(&mut self, format: MTLPixelFormat, width: u32, height: u32) -> bool
   {
      let Some(id) = self.layers.iter()
         .find(|(id, layer)| {
            !self.layer_frame_ids.contains(id)
               && layer.w == width
               && layer.h == height
               && layer.tex.pixel_format() == format
         })
         .map(|(id, _)| *id) else { return false };
      if let Some(layer) = self.layers.remove(&id)
      {
         self.recycle_layer_entry(layer);
         self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
      }
      true
   }

   fn acquire_layer_texture(&mut self, format: MTLPixelFormat, width: u32, height: u32) -> Option<(Texture, u64)>
   {
      if let Some(texture) = self.take_pooled_layer_texture(format, width, height)
      {
         return Some(texture);
      }
      if self.recycle_compatible_unprotected_layer(format, width, height)
      {
         if let Some(texture) = self.take_pooled_layer_texture(format, width, height)
         {
            return Some(texture);
         }
      }
      let required = self.layer_texture_required_bytes(format, width, height);
      if !self.admit_layer_bytes(required)
      {
         return None;
      }
      let descriptor = Self::layer_texture_descriptor(format, width, height);
      let texture = self.device.new_texture(&descriptor);
      let bytes = Self::texture_allocated_bytes(&texture);
      self.layer_cache_resident_bytes = self.layer_cache_resident_bytes.saturating_add(bytes);
      self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
      self.acc_layer_texture_creates = self.acc_layer_texture_creates.saturating_add(1);
      Some((texture, bytes))
   }

   fn age_layer_cache(&mut self)
   {
      if self.frame_id >= LAYER_CACHE_ABSENT_FRAMES
      {
         let absent_before = self.frame_id - LAYER_CACHE_ABSENT_FRAMES;
         loop
         {
            let Some(id) = self.layers.iter()
               .find(|(id, layer)| {
                  !self.layer_frame_ids.contains(id) && layer.last_used_frame <= absent_before
               })
               .map(|(id, _)| *id) else { break };
            if let Some(layer) = self.layers.remove(&id)
            {
               self.recycle_layer_entry(layer);
            }
         }
      }
      if self.frame_id >= LAYER_CACHE_POOL_MAX_AGE_FRAMES
      {
         let pool_before = self.frame_id - LAYER_CACHE_POOL_MAX_AGE_FRAMES;
         while let Some(index) = self.layer_pool.iter()
            .position(|entry| entry.recycled_frame <= pool_before)
         {
            let removed = self.layer_pool.swap_remove(index);
            self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_sub(removed.bytes);
            self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
         }
      }
      self.enforce_layer_cache_budget();
   }

   /// Returns the current renderer-owned generation for an image or glyph atlas.
   pub fn image_generation(&self, handle: api::ImageHandle) -> Option<u64>
   {
      self.image_generations.get(&handle.0).copied()
   }

   fn invalidate_prepared_layers_for_resource(&mut self, handle: api::ImageHandle)
   {
      for layer in self.layers.values_mut()
      {
         if layer.prepared_key.is_some()
            && layer.resources.iter().any(|dependency| dependency.image == handle)
         {
            layer.prepared_key = None;
            layer.resources.clear();
         }
      }
   }

    #[allow(dead_code)]
    fn ensure_scene3d_bloom_targets(&mut self, downsample_divisor: u32) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        let divisor = downsample_divisor.clamp(1, 8);
        let w = ((self.target_w / divisor).max(1)) as u64;
        let h = ((self.target_h / divisor).max(1)) as u64;
        let need_src = match &self.scene3d_bloom_tex {
            Some(tex) => tex.width() != w || tex.height() != h,
            None => true,
        };
        let need_tmp = match &self.scene3d_bloom_tmp_tex {
            Some(tex) => tex.width() != w || tex.height() != h,
            None => true,
        };
        if need_src {
            let d = TextureDescriptor::new();
            d.set_pixel_format(MTLPixelFormat::RGBA16Float);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(w);
            d.set_height(h);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.scene3d_bloom_tex = Some(self.device.new_texture(&d));
            self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
        }
        if need_tmp {
            let d = TextureDescriptor::new();
            d.set_pixel_format(MTLPixelFormat::RGBA16Float);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(w);
            d.set_height(h);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.scene3d_bloom_tmp_tex = Some(self.device.new_texture(&d));
            self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
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
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
        self.last_stats.texture_upload_bytes = self
            .last_stats
            .texture_upload_bytes
            .saturating_add(data.len() as u64);
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.images.insert(id, tex);
        self.image_generations.insert(id, 1);
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
            self.last_stats.texture_upload_bytes = self
                .last_stats
                .texture_upload_bytes
                .saturating_add(data.len() as u64);
            self.prepared_chunks.invalidate_resource(handle);
            self.invalidate_prepared_layers_for_resource(handle);
            let generation = self.image_generations.entry(handle.0).or_insert(0);
            *generation = generation.saturating_add(1);
        }
    }

    /// Publishes bytes for atlas slots that have never previously been sampled.
    /// Existing prepared chunks remain valid because prior texels are unchanged.
    pub fn image_append_a8(
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
            self.last_stats.texture_upload_bytes = self
                .last_stats
                .texture_upload_bytes
                .saturating_add(u64::from(w).saturating_mul(u64::from(h)));
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
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
        self.last_stats.texture_upload_bytes = self
            .last_stats
            .texture_upload_bytes
            .saturating_add(data.len() as u64);
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.images.insert(id, tex);
        self.image_generations.insert(id, 1);
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
            self.last_stats.texture_upload_bytes = self
                .last_stats
                .texture_upload_bytes
                .saturating_add(data.len() as u64);
            self.prepared_chunks.invalidate_resource(handle);
            self.invalidate_prepared_layers_for_resource(handle);
            let generation = self.image_generations.entry(handle.0).or_insert(0);
            *generation = generation.saturating_add(1);
        }
    }

    pub fn image_release(&mut self, handle: api::ImageHandle) {
        if self.images.remove(&handle.0).is_some() {
            self.prepared_chunks.invalidate_resource(handle);
            self.invalidate_prepared_layers_for_resource(handle);
            self.image_generations.remove(&handle.0);
            self.last_stats.cache_evictions = self.last_stats.cache_evictions.saturating_add(1);
        }
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
            && direct_preview_submission_backpressure_applies(
                experimental_preview_submission_cap(),
                self.current_preview_submission_depth(),
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
            if let Some(sam) = &self.sampler {
                enc.set_fragment_sampler_state(0, Some(sam));
            }
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
            self.frame_present_direct_to_drawable = false;
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

    pub fn require_offscreen_present_for_frame(&mut self) {
        self.pending_present_texture = 0;
        self.frame_present_direct_to_drawable = false;
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn create_direct_present_texture_for_snapshot(&self) -> Texture
    {
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(self.color_format);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(self.target_w as u64);
        desc.set_height(self.target_h as u64);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        self.device.new_texture(&desc)
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn prepare_direct_present_texture_for_snapshot(&mut self, texture: &TextureRef)
    {
        self.pending_present_texture = texture.as_ptr() as usize;
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn readback_direct_present_texture_for_snapshot(&self, texture: &TextureRef) -> Option<(u32, u32, alloc::vec::Vec<u8>)>
    {
        self.readback_texture_bgra8(texture)
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn frame_uses_direct_present_for_snapshot(&self) -> bool
    {
        self.frame_present_direct_to_drawable
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn set_force_exact_blur_for_snapshot(&mut self, force_exact: bool)
    {
        self.force_exact_blur_for_snapshot = force_exact;
    }

    #[inline(always)]
    fn paired_blur_allowed(&self) -> bool
    {
        #[cfg(feature = "snapshot-tests")]
        {
            return !self.force_exact_blur_for_snapshot;
        }
        #[cfg(not(feature = "snapshot-tests"))]
        true
    }

    pub fn cancel_present_drawable(&mut self) -> *mut core::ffi::c_void {
        let drawable = self.pending_present_drawable as *mut core::ffi::c_void;
        self.pending_present_drawable = 0;
        self.pending_present_texture = 0;
        self.frame_present_direct_to_drawable = false;
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
        if !drawable_ptr.is_null() && self.sample_count == 1 {
            self.persistent_target_valid = false;
        }
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
        self.layer_frame_ids.clear();
        self.id_mask_frame_cache_serials.clear();
        if self.id_mask_cache_resident_bytes > self.id_mask_cache_budget_bytes
        {
            self.enforce_id_mask_cache_budget();
        }
        self.acc_draws = 0;
        self.acc_flat_instanced_draws = 0;
        self.acc_instanced = 0;
        self.acc_analytic_instance_bytes = 0;
        self.acc_analytic_instance_buffer_binds = 0;
        self.acc_analytic_instance_ring_grows = 0;
        self.acc_scene3d_draws = 0;
        self.acc_scene3d_instances = 0;
        self.acc_scene3d_instance_bytes = 0;
        self.acc_scene3d_pipeline_binds = 0;
        self.acc_scene3d_depth_state_binds = 0;
        self.acc_scene3d_cull_sets = 0;
        self.acc_scene3d_mesh_buffer_binds = 0;
        self.acc_scene3d_instance_buffer_binds = 0;
        self.acc_scene3d_instance_ring_grows = 0;
        self.acc_scene3d_viewport_sets = 0;
        self.acc_glyph_instance_bytes = 0;
        self.acc_glyph_instance_buffer_binds = 0;
        self.acc_glyph_instances = 0;
        self.acc_icb_cmds = 0;
        self.img_arg_used = 0;
        self.image_arg_table_index.clear();
        self.image_arg_table_count = 0;
        if self.accounting_stats_enabled {
            self.acc_commands_traversed = 0;
            self.acc_commands_copied = 0;
            self.acc_geometry_bytes_copied = 0;
            self.acc_chunks_reused = 0;
            self.acc_chunks_rebuilt = 0;
            self.acc_chunks_prepared = 0;
            self.acc_prepared_plan_reuses = 0;
            self.acc_backend_cache_hits = 0;
            self.acc_backend_cache_misses = 0;
            self.acc_damage_instances_visited = 0;
            self.acc_damage_instances_matched = 0;
            self.acc_damage_commands_visited = 0;
            self.acc_damage_commands_matched = 0;
            self.acc_damage_vertices_visited = 0;
            self.acc_damage_query_ns = 0;
            self.acc_layer_body_commands_scanned = 0;
            self.acc_layer_body_commands_copied = 0;
            self.acc_layer_texture_creates = 0;
            self.acc_layer_cache_hits = 0;
            self.acc_layer_cache_misses = 0;
            self.acc_layer_offscreen_draws = 0;
            self.acc_layer_inline_draws = 0;
            self.acc_layer_double_render_prevented = 0;
            self.acc_id_mask_cache_hits = 0;
            self.acc_id_mask_cache_misses = 0;
            self.acc_id_mask_raster_passes = 0;
            self.acc_id_mask_field_seed_passes = 0;
            self.acc_id_mask_field_jump_passes = 0;
            self.acc_id_mask_compositor_passes = 0;
            self.acc_id_mask_target_creates = 0;
            self.acc_image_argument_encodes = 0;
            self.acc_image_argument_binds = 0;
            self.acc_image_argument_tables_finalized = 0;
            self.acc_image_argument_table_reuses = 0;
            self.acc_image_argument_bytes = 0;
            self.acc_image_argument_buffer_grows = 0;
            self.acc_render_passes = 0;
            self.acc_blit_passes = 0;
            self.acc_texture_copies = 0;
            self.acc_texture_copy_pixels = 0;
            self.acc_texture_copy_bytes = 0;
            self.acc_resource_creates = 0;
            self.acc_resource_grows = 0;
        }
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
            let mut preview = CameraPreviewRenderResult::default();
            if let Some(frame) = current_frame.as_ref() {
                preview.camera_width = frame.width;
                preview.camera_height = frame.height;
                preview.camera_bit_depth = frame.bit_depth;
                preview.camera_matrix = frame.matrix;
                preview.camera_video_range = frame.video_range;
                preview.camera_color_space = frame.color_space;
            }
            let backpressure_blocked = self.direct_preview_backpressure_blocks_present();
            if !backpressure_blocked && !drawable_ptr.is_null() {
                preview = self
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
                self.note_direct_preview_submission_depth();
                self.direct_preview_last_present_frame_age_ms = preview.present_frame_age_ms;
            }
            self.last_cam_w = preview.camera_width.max(0);
            self.last_cam_h = preview.camera_height.max(0);
            self.last_cam_bd = preview.camera_bit_depth.max(0);
            self.last_cam_mx = preview.camera_matrix.max(0);
            self.last_cam_vr = preview.camera_video_range.max(0);
            self.last_cam_cs = preview.camera_color_space.max(0);
            self.acc_draws = if preview.drew_live_frame { 1 } else { 0 };
            self.last_stats = PerfStats {
                memory: self.collected_memory_stats(),
                draws: self.acc_draws,
                instanced: self.acc_instanced,
                icb_cmds: self.acc_icb_cmds,
                commands_traversed: self.acc_draws as u64,
                render_passes: self.acc_draws,
                command_buffers: self.acc_draws,
                shaded_damage_px: if preview.drew_live_frame {
                    u64::from(w).saturating_mul(u64::from(h))
                } else {
                    0
                },
                skipped_submissions: self.direct_preview_last_submission_skipped,
                actual_submissions: (self.direct_preview_last_submission_skipped == 0) as u32,
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
                cam_command_buffer_ms: preview.command_buffer_ms,
                cam_encoder_ms: preview.encoder_ms,
                cam_encode_bind_ms: 0.0,
                cam_encode_draw_ms: 0.0,
                cam_end_encoding_ms: 0.0,
                cam_present_ms: preview.present_ms,
                cam_commit_ms: preview.commit_ms,
                cam_gpu_ms: 0.0,
                cam_gpu_render_ms: 0.0,
                cam_gpu_vertex_ms: 0.0,
                cam_gpu_fragment_ms: 0.0,
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
                let direct_preview_gpu_trace = self
                    .gpu_stage_timing
                    .as_ref()
                    .and_then(|timing| timing.begin_submission(&self.device));
                let should_clear = direct_preview_should_clear_load_action(
                    direct_preview_uses_dontcare_load_action(),
                    current_frame.is_some(),
                );
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
                        if should_clear {
                            ca0.set_load_action(MTLLoadAction::Clear);
                            ca0.set_clear_color(MTLClearColor {
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                                alpha: 1.0,
                            });
                        } else {
                            ca0.set_load_action(MTLLoadAction::DontCare);
                        }
                        if let Some(gpu_trace) = direct_preview_gpu_trace.as_ref() {
                            gpu_trace.configure_render_pass(&rpd);
                        }
                        Ok(())
                    },
                )?;
                setup_ms = elapsed_ms(setup_t0);
                let encoder_t0 = collect_stage_stats.then(Instant::now);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
                self.track_direct_preview_submission(
                    self.frame_id,
                    current_frame.as_ref().map(|frame| frame.generation).unwrap_or(0),
                    &cmd,
                    direct_preview_gpu_trace,
                );
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
                memory: self.collected_memory_stats(),
                draws: self.acc_draws,
                instanced: self.acc_instanced,
                icb_cmds: self.acc_icb_cmds,
                commands_traversed: self.acc_draws as u64,
                render_passes: self.acc_render_passes,
                command_buffers: 1,
                shaded_damage_px: if drew_live_frame {
                    u64::from(w).saturating_mul(u64::from(h))
                } else {
                    0
                },
                skipped_submissions: self.direct_preview_last_submission_skipped,
                actual_submissions: (self.direct_preview_last_submission_skipped == 0) as u32,
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
                cam_gpu_render_ms: self.direct_preview_last_completed_gpu_render_ms,
                cam_gpu_vertex_ms: self.direct_preview_last_completed_gpu_vertex_ms,
                cam_gpu_fragment_ms: self.direct_preview_last_completed_gpu_fragment_ms,
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
        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
        enc.end_encoding();
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
        self.track_direct_preview_submission(self.frame_id, 0, &cmd, None);
        self.last_stats = PerfStats {
            memory: self.collected_memory_stats(),
            draws: self.acc_draws,
            instanced: self.acc_instanced,
            icb_cmds: self.acc_icb_cmds,
            commands_traversed: self.acc_draws as u64,
            render_passes: self.acc_render_passes,
            command_buffers: 1,
            shaded_damage_px: if camera_props.is_some() {
                u64::from(w).saturating_mul(u64::from(h))
            } else {
                0
            },
            actual_submissions: 1,
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
            cam_gpu_render_ms: self.direct_preview_last_completed_gpu_render_ms,
            cam_gpu_vertex_ms: self.direct_preview_last_completed_gpu_vertex_ms,
            cam_gpu_fragment_ms: self.direct_preview_last_completed_gpu_fragment_ms,
            preview_submission_depth: self.direct_preview_last_submission_depth,
            preview_submission_skipped: self.direct_preview_last_submission_skipped,
            preview_submission_frame_age_ms: self.direct_preview_last_present_frame_age_ms,
            ..PerfStats::default()
        };
        Ok(self.last_stats)
    }

    fn readback_texture_bytes(
        &self,
        tex: &TextureRef,
        bytes_per_pixel: usize,
    ) -> Option<(u32, u32, alloc::vec::Vec<u8>)> {
        let w = tex.width() as u32;
        let h = tex.height() as u32;
        if w == 0 || h == 0 || bytes_per_pixel == 0 {
            return None;
        }
        let row_bytes = (w as usize).checked_mul(bytes_per_pixel)?;
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
            buf_bytes as u64,
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

    fn readback_texture_bgra8(&self, tex: &TextureRef) -> Option<(u32, u32, alloc::vec::Vec<u8>)> {
        self.readback_texture_bytes(tex, 4)
    }

    fn readback_direct_live_camera_bgra8(&self) -> Option<(u32, u32, alloc::vec::Vec<u8>)> {
        let frame = self.current_live_camera_frame.as_ref()?;
        let w = if self.target_w > 0 { self.target_w } else { frame.width.max(1) as u32 };
        let h = if self.target_h > 0 { self.target_h } else { frame.height.max(1) as u32 };
        let scale = self.target_scale.max(1.0);
        let vp_dp = [(w as f32) / scale, (h as f32) / scale];
        let rect_dp = [0.0, 0.0, vp_dp[0], vp_dp[1]];

        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        let tex = self.device.new_texture(&desc);
        let cmd = self.queue.new_command_buffer();
        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        ca0.set_texture(Some(&tex));
        ca0.set_load_action(MTLLoadAction::Clear);
        let clear_alpha = if transparent_drawable_clear_enabled() { 0.0 } else { 1.0 };
        ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: clear_alpha });
        ca0.set_store_action(MTLStoreAction::Store);
        let enc = cmd.new_render_command_encoder(&rpd);
        self.encode_camera_quad_from_live_frame(
            &enc,
            frame,
            vp_dp,
            rect_dp,
            api::Color::rgba(1.0, 1.0, 1.0, 1.0),
            1.0,
            false,
            false,
        );
        enc.end_encoding();

        let row_bytes = (w as usize) * 4;
        let buf_bytes = row_bytes * (h as usize);
        let opts =
            MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeShared;
        let buf = self.device.new_buffer(buf_bytes as u64, opts);
        let blit = cmd.new_blit_command_encoder();
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let size = MTLSize { width: w as u64, height: h as u64, depth: 1 };
        blit.copy_from_texture_to_buffer(
            &tex,
            0,
            0,
            origin,
            size,
            &buf,
            0,
            row_bytes as u64,
            buf_bytes as u64,
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

    pub fn readback_bgra8(&mut self) -> Option<(u32, u32, alloc::vec::Vec<u8>)> {
        if self.color_format != MTLPixelFormat::BGRA8Unorm_sRGB {
            return None;
        }
        if let Some(tex) = self.target_tex.as_ref() {
            return self.readback_texture_bgra8(tex.as_ref());
        }
        self.readback_direct_live_camera_bgra8()
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn readback_color_snapshot(&self) -> Option<MetalSnapshotColorReadback>
    {
        let texture = self.target_tex.as_ref()?;
        let (format, bytes_per_pixel) = match self.color_format {
            MTLPixelFormat::BGRA8Unorm_sRGB => (MetalSnapshotColorFormat::Bgra8Srgb, 4),
            MTLPixelFormat::BGRA10_XR => (MetalSnapshotColorFormat::Bgra10Xr, 8),
            _ => return None,
        };
        let (width, height, bytes) =
            self.readback_texture_bytes(texture.as_ref(), bytes_per_pixel)?;
        Some(MetalSnapshotColorReadback { width, height, format, bytes })
    }

    fn validate_mesh3d_upload(
        vertex_count: usize,
        indices: &[u32],
        topology: scene3d::MeshTopology,
        colored: bool,
    ) -> Result<(), api::RenderError> {
        if vertex_count == 0 {
            return Err(api::RenderError::InvalidOperation(if colored {
                "mesh3d_create_colored requires vertices"
            } else {
                "mesh3d_create requires vertices"
            }));
        }
        if indices.is_empty() {
            return Err(api::RenderError::InvalidOperation(if colored {
                "mesh3d_create_colored requires indices"
            } else {
                "mesh3d_create requires indices"
            }));
        }
        match topology {
            scene3d::MeshTopology::Triangles if indices.len() % 3 != 0 => {
                return Err(api::RenderError::InvalidOperation(if colored {
                    "triangle colored mesh indices must be a multiple of 3"
                } else {
                    "triangle mesh indices must be a multiple of 3"
                }));
            }
            scene3d::MeshTopology::Lines if colored => {
                return Err(api::RenderError::InvalidOperation(
                    "colored scene3d mesh only supports triangles",
                ));
            }
            scene3d::MeshTopology::Lines if indices.len() % 2 != 0 => {
                return Err(api::RenderError::InvalidOperation(
                    "line mesh indices must be a multiple of 2",
                ));
            }
            _ => {}
        }
        let mut max_index = 0_u32;
        for &index in indices {
            max_index = max_index.max(index);
        }
        if max_index as usize >= vertex_count {
            return Err(api::RenderError::InvalidOperation(if colored {
                "colored mesh index referenced a vertex outside the provided slice"
            } else {
                "mesh index referenced a vertex outside the provided slice"
            }));
        }
        Ok(())
    }

    fn upload_mesh3d_buffers<T>(
        &self,
        vertices: &[T],
        indices: &[u32],
    ) -> Result<(Buffer, Buffer), api::RenderError> {
        let vb_len = (vertices.len() * core::mem::size_of::<T>()) as u64;
        let ib_len = (indices.len() * core::mem::size_of::<u32>()) as u64;
        let vb = self.device.new_buffer(vb_len, MTLResourceOptions::StorageModeShared);
        let ib = self.device.new_buffer(ib_len, MTLResourceOptions::StorageModeShared);
        let vb_ptr = vb.contents();
        let ib_ptr = ib.contents();
        if vb_ptr.is_null() || ib_ptr.is_null() {
            return Err(api::RenderError::OutOfMemory);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                vertices.as_ptr() as *const u8,
                vb_ptr as *mut u8,
                vb_len as usize,
            );
            core::ptr::copy_nonoverlapping(
                indices.as_ptr() as *const u8,
                ib_ptr as *mut u8,
                ib_len as usize,
            );
        }
        Ok((vb, ib))
    }

    fn insert_mesh3d(
        &mut self,
        vb: Buffer,
        ib: Buffer,
        index_count: usize,
        topology: scene3d::MeshTopology,
        format: MeshFormat3d,
        opaque: bool,
    ) -> scene3d::MeshHandle3d {
        let id = self.next_mesh3d_id;
        self.next_mesh3d_id = self.next_mesh3d_id.wrapping_add(1).max(1);
        self.meshes_3d
            .insert(id, Mesh3dGpu {
                vb,
                ib,
                index_count: index_count as u64,
                topology,
                format,
                opaque,
            });
        scene3d::MeshHandle3d(id)
    }

    /// Uploads a static indexed 3D mesh into persistent Metal buffers.
    pub fn mesh3d_create(
        &mut self,
        data: &scene3d::Mesh3dData<'_>,
    ) -> Result<scene3d::MeshHandle3d, api::RenderError> {
        Self::validate_mesh3d_upload(data.vertices.len(), data.indices, data.topology, false)?;
        let (vb, ib) = self.upload_mesh3d_buffers(data.vertices, data.indices)?;
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(2);
        self.last_stats.buffer_upload_bytes = self
            .last_stats
            .buffer_upload_bytes
            .saturating_add(vb.length())
            .saturating_add(ib.length());
        Ok(self.insert_mesh3d(
            vb,
            ib,
            data.indices.len(),
            data.topology,
            MeshFormat3d::Position,
            true,
        ))
    }

    /// Uploads a static indexed colored 3D mesh into persistent Metal buffers.
    pub fn mesh3d_create_colored(
        &mut self,
        data: &scene3d::MeshColor3dData<'_>,
    ) -> Result<scene3d::MeshHandle3d, api::RenderError> {
        Self::validate_mesh3d_upload(data.vertices.len(), data.indices, data.topology, true)?;
        let (vb, ib) = self.upload_mesh3d_buffers(data.vertices, data.indices)?;
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(2);
        self.last_stats.buffer_upload_bytes = self
            .last_stats
            .buffer_upload_bytes
            .saturating_add(vb.length())
            .saturating_add(ib.length());
        Ok(self.insert_mesh3d(
            vb,
            ib,
            data.indices.len(),
            data.topology,
            MeshFormat3d::PositionColor,
            data.vertices.iter().all(|vertex| vertex.color[3] >= 1.0),
        ))
    }

    /// Releases a previously uploaded 3D mesh handle.
    pub fn mesh3d_release(&mut self, handle: scene3d::MeshHandle3d) {
        let _ = self.meshes_3d.remove(&handle.0);
    }

    /// Encodes one scene3d pass into the current frame.
    ///
    /// Scene3D and ID-mask passes may be interleaved with 2D passes so app
    /// shells can embed shared renderers at the correct
    /// draw-list depth without forcing the whole frame into that renderer.
    pub fn encode_scene3d(&mut self, pass: &scene3d::Pass3d<'_>) -> Result<(), api::RenderError> {
        if self.frame_backpressure_skipped {
            return Ok(());
        }
        if self.sample_count != 1 {
            return Err(api::RenderError::Unsupported(
                "scene3d currently requires MetalRenderer sample_count == 1",
            ));
        }
        if pass.viewport.is_some() && pass.bloom.is_some() {
            return Err(api::RenderError::Unsupported(
                "scene3d viewport clipping is not implemented for bloom",
            ));
        }

        self.ensure_target();
        self.ensure_depth_target();
        let slot = self.current_frame_slot();
        let cmd = self.ensure_frame_command_buffer(slot);
        let Some(target_tex) = self.target_tex.as_ref().map(Texture::to_owned) else {
            return Err(api::RenderError::InvalidOperation("scene3d target texture unavailable"));
        };
        let Some(depth_tex) = self.depth_tex.as_ref().map(Texture::to_owned) else {
            return Err(api::RenderError::InvalidOperation("scene3d depth texture unavailable"));
        };
        let mut pf = core::mem::take(&mut self.frames[slot]);
        let prepared = self.prepare_scene3d_draws(
            &mut pf,
            pass.view_proj,
            pass.instances,
            1.0,
            false,
        );
        let (uniform_offset, material_offset) = match prepared {
            Ok(offsets) => offsets,
            Err(error) => {
                self.frames[slot] = pf;
                return Err(error);
            }
        };
        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        ca0.set_texture(Some(&target_tex));
        ca0.set_store_action(MTLStoreAction::Store);
        if self.frame_color_initialized && self.persistent_target_valid {
            ca0.set_load_action(MTLLoadAction::Load);
        } else if let Some(color) = pass.clear_color {
            ca0.set_load_action(MTLLoadAction::Clear);
            ca0.set_clear_color(MTLClearColor {
                red: color.r as f64,
                green: color.g as f64,
                blue: color.b as f64,
                alpha: color.a as f64,
            });
        } else {
            ca0.set_load_action(MTLLoadAction::Clear);
            ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0 });
        }

        let da = rpd.depth_attachment().unwrap();
        da.set_texture(Some(&depth_tex));
        da.set_store_action(MTLStoreAction::Store);
        if self.frame_depth_initialized && !pass.clear_depth {
            da.set_load_action(MTLLoadAction::Load);
        } else {
            da.set_load_action(MTLLoadAction::Clear);
            da.set_clear_depth(1.0);
        }

        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        let enc = cmd.new_render_command_encoder(&rpd);
        enc.set_front_facing_winding(MTLWinding::CounterClockwise);
        if let Some(viewport) = pass.viewport {
            set_viewport_and_scissor_dp(&enc, self, viewport);
            self.acc_scene3d_viewport_sets = self.acc_scene3d_viewport_sets.saturating_add(1);
        }
        self.encode_prepared_scene3d_draws(&enc, uniform_offset, material_offset);
        enc.end_encoding();
        let bloom_result = if let Some(bloom) = pass.bloom {
            self.encode_scene3d_bloom(&cmd, &target_tex, &mut pf, pass.view_proj, bloom)
        } else {
            Ok(())
        };
        self.frames[slot] = pf;
        bloom_result?;
        self.frame_color_initialized = true;
        self.persistent_target_valid = true;
        self.frame_depth_initialized = true;
        if let Some(t0) = self.frame_encode_started_at {
            self.last_stats.encode_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
        self.last_stats.draws = self.acc_draws.saturating_add(self.acc_flat_instanced_draws);
        self.last_stats.instanced = self.acc_instanced;
        self.last_stats.scene3d_draws = self.acc_scene3d_draws;
        self.last_stats.scene3d_instances = self.acc_scene3d_instances;
        self.last_stats.scene3d_instance_bytes = self.acc_scene3d_instance_bytes;
        self.last_stats.scene3d_pipeline_binds = self.acc_scene3d_pipeline_binds;
        self.last_stats.scene3d_depth_state_binds = self.acc_scene3d_depth_state_binds;
        self.last_stats.scene3d_cull_sets = self.acc_scene3d_cull_sets;
        self.last_stats.scene3d_mesh_buffer_binds = self.acc_scene3d_mesh_buffer_binds;
        self.last_stats.scene3d_instance_buffer_binds = self.acc_scene3d_instance_buffer_binds;
        self.last_stats.scene3d_instance_ring_grows = self.acc_scene3d_instance_ring_grows;
        self.last_stats.scene3d_viewport_sets = self.acc_scene3d_viewport_sets;
        Ok(())
    }

    fn encode_scene3d_bloom(
        &mut self,
        cmd: &CommandBuffer,
        target_tex: &Texture,
        pf: &mut PerFrame,
        view_proj: scene3d::Mat4,
        bloom: scene3d::Bloom3d<'_>,
    ) -> Result<(), api::RenderError> {
        if bloom.emissive_instances.is_empty()
            || !bloom.layers.iter().any(|layer| layer.strength > 0.0 && layer.sigma_px > 0.0)
        {
            return Ok(());
        }
        let divisor = bloom.downsample_divisor.clamp(1, 8);
        self.ensure_scene3d_bloom_targets(divisor);
        let Some(bloom_tex) = self.scene3d_bloom_tex.as_ref().map(Texture::to_owned) else {
            return Ok(());
        };
        let Some(bloom_tmp_tex) = self.scene3d_bloom_tmp_tex.as_ref().map(Texture::to_owned) else {
            return Ok(());
        };
        let (uniform_offset, material_offset) = self.prepare_scene3d_draws(
            pf,
            view_proj,
            bloom.emissive_instances,
            1.0,
            true,
        )?;
        for layer in bloom.layers {
            if layer.strength <= 0.0 || layer.sigma_px <= 0.0 {
                continue;
            }
            let rpd = RenderPassDescriptor::new();
            let ca = rpd.color_attachments().object_at(0).unwrap();
            ca.set_texture(Some(&bloom_tex));
            ca.set_load_action(MTLLoadAction::Clear);
            ca.set_store_action(MTLStoreAction::Store);
            ca.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0 });
            self.acc_render_passes = self.acc_render_passes.saturating_add(1);
            let enc = cmd.new_render_command_encoder(&rpd);
            enc.set_front_facing_winding(MTLWinding::CounterClockwise);
            self.encode_prepared_scene3d_draws(&enc, uniform_offset, material_offset);
            enc.end_encoding();

            let pass_sigma = (layer.sigma_px / divisor as f32).max(0.75);
            let pass_radius = (pass_sigma * 3.0).ceil().clamp(2.0, 192.0);
            self.encode_scene3d_bloom_blur_pass(
                cmd,
                &bloom_tex,
                &bloom_tmp_tex,
                [1.0, 0.0, pass_sigma, pass_radius],
            );
            self.encode_scene3d_bloom_blur_pass(
                cmd,
                &bloom_tmp_tex,
                &bloom_tex,
                [0.0, 1.0, pass_sigma, pass_radius],
            );
            self.encode_scene3d_bloom_composite(cmd, &bloom_tex, target_tex, layer.strength);
        }
        Ok(())
    }

    fn encode_scene3d_bloom_blur_pass(
        &mut self,
        cmd: &CommandBuffer,
        src: &Texture,
        dst: &Texture,
        params: [f32; 4],
    ) {
        let rpd = RenderPassDescriptor::new();
        let ca = rpd.color_attachments().object_at(0).unwrap();
        ca.set_texture(Some(dst));
        ca.set_load_action(MTLLoadAction::DontCare);
        ca.set_store_action(MTLStoreAction::Store);
        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        let enc = cmd.new_render_command_encoder(&rpd);
        if let Some(sam) = &self.sampler {
            enc.set_fragment_sampler_state(0, Some(sam));
        }
        enc.set_fragment_texture(0, Some(src));
        let kernel = bind_blur_kernel(
            &enc,
            params,
            self.paired_blur_allowed(),
            &self.pso_bloom_blur,
            &self.pso_bloom_blur_paired,
        );
        record_blur_kernel_use(&mut self.last_stats, kernel);
        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
        enc.end_encoding();
        self.acc_draws = self.acc_draws.saturating_add(1);
    }

    fn encode_scene3d_bloom_composite(
        &mut self,
        cmd: &CommandBuffer,
        src: &Texture,
        dst: &Texture,
        strength: f32,
    ) {
        let rpd = RenderPassDescriptor::new();
        let ca = rpd.color_attachments().object_at(0).unwrap();
        ca.set_texture(Some(dst));
        ca.set_load_action(MTLLoadAction::Load);
        ca.set_store_action(MTLStoreAction::Store);
        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        let enc = cmd.new_render_command_encoder(&rpd);
        enc.set_render_pipeline_state(&self.pso_bloom_composite);
        if let Some(sam) = &self.sampler {
            enc.set_fragment_sampler_state(0, Some(sam));
        }
        let strength = strength.max(0.0);
        enc.set_fragment_texture(0, Some(src));
        enc.set_fragment_bytes(
            1,
            core::mem::size_of_val(&strength) as u64,
            (&strength as *const f32).cast(),
        );
        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
        enc.end_encoding();
        self.acc_draws = self.acc_draws.saturating_add(1);
    }

    fn prepare_scene3d_draws(
        &mut self,
        pf: &mut PerFrame,
        view_proj: scene3d::Mat4,
        instances: &[scene3d::Instance3d],
        intensity: f32,
        bloom_target: bool,
    ) -> Result<(usize, usize), api::RenderError> {
        self.scene3d_draws.clear();
        if instances.is_empty() {
            return Ok((0, 0));
        }
        let count = instances.len();
        let _ = u32::try_from(count).map_err(|_| {
            api::RenderError::InvalidOperation("scene3d instance count exceeds Metal limits")
        })?;
        let (uniform_offset, material_offset, end) =
            analytic_instance_pair_layout::<Scene3dGpuUniforms, Scene3dGpuMaterial>(
                pf.ub_used,
                count,
            );
        let slot = self.current_frame_slot();
        if self.ub.ensure_capacity(&self.device, slot, end) {
            self.acc_resource_grows = self.acc_resource_grows.saturating_add(1);
            self.acc_scene3d_instance_ring_grows =
                self.acc_scene3d_instance_ring_grows.saturating_add(1);
        }
        pf.ub_used = end;
        self.acc_scene3d_instance_bytes = self.acc_scene3d_instance_bytes.saturating_add(
            count.saturating_mul(
                core::mem::size_of::<Scene3dGpuUniforms>()
                    + core::mem::size_of::<Scene3dGpuMaterial>(),
            ) as u64,
        );

        let base = self.ub.contents_ptr(slot);
        let color_scale = intensity.max(0.0);
        for (index, instance) in instances.iter().enumerate() {
            let Some(mesh) = self.meshes_3d.get(&instance.mesh.0) else {
                return Err(api::RenderError::ResourceNotFound("mesh3d handle"));
            };
            let (pipeline, depth, batchable) =
                scene3d_draw_configuration(mesh, instance, bloom_target)?;
            let uniforms = Scene3dGpuUniforms {
                mvp: scene3d::mat4_mul(&view_proj, &instance.transform),
            };
            let material = Scene3dGpuMaterial {
                color: [
                    instance.color.r * color_scale,
                    instance.color.g * color_scale,
                    instance.color.b * color_scale,
                    instance.color.a,
                ],
                material: scene3d_material_id(instance.material),
                _pad: [0.0; 3],
                params: instance.params,
            };
            unsafe {
                write_ring_value(base, uniform_offset, index, uniforms);
                write_ring_value(base, material_offset, index, material);
            }

            let first_instance = index as u32;
            if batchable
                && self.scene3d_draws.last().is_some_and(|draw| {
                    draw.batchable
                        && draw.mesh == instance.mesh.0
                        && draw.pipeline == pipeline
                        && draw.depth == depth
                        && draw.cull == instance.cull
                        && draw.first_instance.saturating_add(draw.instance_count)
                            == first_instance
                })
            {
                let draw = self.scene3d_draws.last_mut().unwrap();
                draw.instance_count = draw.instance_count.saturating_add(1);
            } else {
                self.scene3d_draws.push(Scene3dDraw {
                    mesh: instance.mesh.0,
                    first_instance,
                    instance_count: 1,
                    pipeline,
                    depth,
                    cull: instance.cull,
                    batchable,
                });
            }
        }
        Ok((uniform_offset, material_offset))
    }

    fn encode_prepared_scene3d_draws(
        &mut self,
        enc: &RenderCommandEncoderRef,
        uniform_offset: usize,
        material_offset: usize,
    ) {
        if self.scene3d_draws.is_empty() {
            return;
        }
        let slot = self.current_frame_slot();
        enc.set_vertex_buffer(1, Some(self.ub.buffer(slot)), uniform_offset as u64);
        enc.set_fragment_buffer(0, Some(self.ub.buffer(slot)), material_offset as u64);
        self.acc_scene3d_instance_buffer_binds =
            self.acc_scene3d_instance_buffer_binds.saturating_add(2);

        let mut active_pipeline = None;
        let mut active_depth = None;
        let mut active_cull = None;
        let mut active_mesh = None;
        for index in 0..self.scene3d_draws.len() {
            let draw = self.scene3d_draws[index];
            if active_pipeline != Some(draw.pipeline) {
                let pipeline = match draw.pipeline {
                    Scene3dPipelineKind::PositionTriAlpha => &self.pso_scene3d_tri,
                    Scene3dPipelineKind::PositionTriAdditive => &self.pso_scene3d_tri_add,
                    Scene3dPipelineKind::PositionTriDepth => &self.pso_scene3d_tri_depth,
                    Scene3dPipelineKind::PositionTriBloom => &self.pso_scene3d_tri_add_bloom,
                    Scene3dPipelineKind::PositionLineAlpha => &self.pso_scene3d_line,
                    Scene3dPipelineKind::PositionLineAdditive => &self.pso_scene3d_line_add,
                    Scene3dPipelineKind::PositionLineDepth => &self.pso_scene3d_line_depth,
                    Scene3dPipelineKind::PositionLineBloom => &self.pso_scene3d_line_add_bloom,
                    Scene3dPipelineKind::ColorTriAlpha => &self.pso_scene3d_color_tri,
                    Scene3dPipelineKind::ColorTriAdditive => &self.pso_scene3d_color_tri_add,
                    Scene3dPipelineKind::ColorTriDepth => &self.pso_scene3d_color_tri_depth,
                };
                enc.set_render_pipeline_state(pipeline);
                self.acc_scene3d_pipeline_binds =
                    self.acc_scene3d_pipeline_binds.saturating_add(1);
                active_pipeline = Some(draw.pipeline);
            }
            if active_depth != Some(draw.depth) {
                let depth = match draw.depth {
                    Scene3dDepthKind::Disabled => &self.depth_state_3d_disabled,
                    Scene3dDepthKind::Read => &self.depth_state_3d_read,
                    Scene3dDepthKind::Write => &self.depth_state_3d_write,
                    Scene3dDepthKind::WriteNoTest => &self.depth_state_3d_write_no_test,
                };
                enc.set_depth_stencil_state(depth);
                self.acc_scene3d_depth_state_binds =
                    self.acc_scene3d_depth_state_binds.saturating_add(1);
                active_depth = Some(draw.depth);
            }
            if active_cull != Some(draw.cull) {
                enc.set_cull_mode(scene3d_cull_mode(draw.cull));
                self.acc_scene3d_cull_sets = self.acc_scene3d_cull_sets.saturating_add(1);
                active_cull = Some(draw.cull);
            }
            let Some(mesh) = self.meshes_3d.get(&draw.mesh) else {
                continue;
            };
            if active_mesh != Some(draw.mesh) {
                enc.set_vertex_buffer(0, Some(&mesh.vb), 0);
                self.acc_scene3d_mesh_buffer_binds =
                    self.acc_scene3d_mesh_buffer_binds.saturating_add(1);
                active_mesh = Some(draw.mesh);
            }
            enc.draw_indexed_primitives_instanced_base_instance(
                scene3d_primitive(mesh.topology),
                mesh.index_count,
                MTLIndexType::UInt32,
                &mesh.ib,
                0,
                draw.instance_count as u64,
                0,
                draw.first_instance as u64,
            );
            self.acc_flat_instanced_draws = self.acc_flat_instanced_draws.saturating_add(1);
            self.acc_instanced = self.acc_instanced.saturating_add(draw.instance_count);
            self.acc_scene3d_draws = self.acc_scene3d_draws.saturating_add(1);
            self.acc_scene3d_instances =
                self.acc_scene3d_instances.saturating_add(draw.instance_count);
        }
    }
}

// Fill reusable draw-command scratch with only items whose bounding rect in dp
// intersects the provided dp scissor. Vertices/indices stay borrowed from the
// source DrawList, so command spans remain valid.
const DARK_POPUP_MAX_BLUR_SIGMA_DP: f32 = 72.0;
// Subthreshold and noncanonical kernels retain the exact shader. Canonical
// kernels use the paired path only when sigma and ceil(3*sigma) match a table
// bucket exactly; the snapshot sweep bounds resulting quantization to 1/255.
const BLUR_KERNEL_BUCKETS_PER_SIGMA: f32 = 16.0;
const BLUR_KERNEL_MAX_SIGMA: f32 = 64.0;
const BLUR_KERNEL_MIN_PAIRED_SIGMA: f32 = 2.0;
const BLUR_KERNEL_MAX_RADIUS: u32 = 192;
const BLUR_KERNEL_BUCKET_COUNT: usize =
    (BLUR_KERNEL_MAX_SIGMA * BLUR_KERNEL_BUCKETS_PER_SIGMA) as usize;
static BLUR_KERNEL_DYNAMIC_BYTES: AtomicU64 = AtomicU64::new(0);

struct BlurKernelEntry {
    radius: u32,
    horizontal: alloc::vec::Vec<f32>,
    vertical: alloc::vec::Vec<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BlurKernelUse {
    paired: bool,
    source_samples: u32,
    encoded_samples: u32,
    runtime_exp_taps: u32,
    table_bytes: u64,
}

fn effective_blur_radius(radius: f32) -> u32 {
    if radius.is_finite() {
        radius.round().clamp(2.0, BLUR_KERNEL_MAX_RADIUS as f32) as u32
    } else {
        2
    }
}

fn build_blur_kernel(bucket: usize) -> BlurKernelEntry {
    let sigma = bucket as f32 / BLUR_KERNEL_BUCKETS_PER_SIGMA;
    let radius = effective_blur_radius((sigma * 3.0).ceil());
    let mut norm = 1.0f32;
    for tap in 1..=radius {
        let x = tap as f32 / sigma;
        norm += 2.0 * (-0.5 * x * x).exp();
    }
    let pair_count = radius.div_ceil(2) as usize;
    let mut horizontal = alloc::vec::Vec::with_capacity(6 + pair_count * 2);
    horizontal.extend_from_slice(&[1.0, 0.0, sigma, radius as f32, 1.0 / norm, pair_count as f32]);
    let mut tap = 1;
    while tap <= radius {
        let x0 = tap as f32 / sigma;
        let weight0 = (-0.5 * x0 * x0).exp();
        let weight1 = if tap < radius {
            let x1 = (tap + 1) as f32 / sigma;
            (-0.5 * x1 * x1).exp()
        } else {
            0.0
        };
        let weight = weight0 + weight1;
        horizontal.extend_from_slice(&[tap as f32 + weight1 / weight, weight / norm]);
        tap += 2;
    }
    let mut vertical = horizontal.clone();
    vertical[0] = 0.0;
    vertical[1] = 1.0;
    BLUR_KERNEL_DYNAMIC_BYTES.fetch_add(
        ((horizontal.capacity() + vertical.capacity()) * core::mem::size_of::<f32>()) as u64,
        Ordering::Relaxed,
    );
    BlurKernelEntry { radius, horizontal, vertical }
}

fn blur_kernel_table_bytes() -> u64 {
    let dynamic = BLUR_KERNEL_DYNAMIC_BYTES.load(Ordering::Relaxed);
    if dynamic == 0 {
        return 0;
    }
    core::mem::size_of::<[OnceLock<BlurKernelEntry>; BLUR_KERNEL_BUCKET_COUNT + 1]>() as u64
        + dynamic
}

fn paired_blur_kernel(sigma: f32, radius: f32, direction: [f32; 2]) -> Option<(&'static [f32], u32)> {
    if !sigma.is_finite() || sigma < BLUR_KERNEL_MIN_PAIRED_SIGMA || sigma > BLUR_KERNEL_MAX_SIGMA {
        return None;
    }
    let scaled = sigma * BLUR_KERNEL_BUCKETS_PER_SIGMA;
    let bucket = scaled.round();
    if (scaled - bucket).abs() > 1.0e-4 {
        return None;
    }
    static TABLE: OnceLock<[OnceLock<BlurKernelEntry>; BLUR_KERNEL_BUCKET_COUNT + 1]> = OnceLock::new();
    let table = TABLE.get_or_init(|| core::array::from_fn(|_| OnceLock::new()));
    let bucket = bucket as usize;
    let entry = table.get(bucket)?.get_or_init(|| build_blur_kernel(bucket));
    if entry.radius != effective_blur_radius(radius) {
        return None;
    }
    let kernel = match direction
    {
        [1.0, 0.0] => entry.horizontal.as_slice(),
        [0.0, 1.0] => entry.vertical.as_slice(),
        _ => return None,
    };
    Some((kernel, ((kernel.len() - 6) / 2) as u32))
}

fn bind_blur_kernel(
    encoder: &RenderCommandEncoderRef,
    params: [f32; 4],
    paired_allowed: bool,
    exact_pso: &RenderPipelineState,
    paired_pso: &RenderPipelineState,
) -> BlurKernelUse {
    let radius = effective_blur_radius(params[3]);
    let paired_kernel = if paired_allowed {
        paired_blur_kernel(params[2], params[3], [params[0], params[1]])
    } else {
        None
    };
    if let Some((kernel, pair_count)) = paired_kernel {
        encoder.set_render_pipeline_state(paired_pso);
        encoder.set_fragment_bytes(
            1,
            core::mem::size_of_val(kernel) as u64,
            kernel.as_ptr().cast(),
        );
        return BlurKernelUse {
            paired: true,
            source_samples: 1 + radius * 2,
            encoded_samples: 1 + pair_count * 2,
            runtime_exp_taps: 0,
            table_bytes: blur_kernel_table_bytes(),
        };
    }
    encoder.set_render_pipeline_state(exact_pso);
    encoder.set_fragment_bytes(
        1,
        core::mem::size_of_val(&params) as u64,
        params.as_ptr().cast(),
    );
    BlurKernelUse {
        paired: false,
        source_samples: 1 + radius * 2,
        encoded_samples: 1 + radius * 2,
        runtime_exp_taps: radius,
        table_bytes: blur_kernel_table_bytes(),
    }
}

fn record_blur_kernel_use(stats: &mut PerfStats, kernel: BlurKernelUse) {
    stats.blur_kernel_paired_passes = stats
        .blur_kernel_paired_passes
        .saturating_add(kernel.paired as u32);
    stats.blur_kernel_exact_passes = stats
        .blur_kernel_exact_passes
        .saturating_add((!kernel.paired) as u32);
    stats.blur_kernel_source_samples = stats
        .blur_kernel_source_samples
        .saturating_add(u64::from(kernel.source_samples));
    stats.blur_kernel_encoded_samples = stats
        .blur_kernel_encoded_samples
        .saturating_add(u64::from(kernel.encoded_samples));
    stats.blur_kernel_runtime_exp_taps = stats
        .blur_kernel_runtime_exp_taps
        .saturating_add(u64::from(kernel.runtime_exp_taps));
    stats.blur_kernel_table_bytes = stats
        .blur_kernel_table_bytes
        .max(kernel.table_bytes);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffectTargetPlan {
    Prepass,
    Quarter,
    Eighth,
}

impl EffectTargetPlan {
    #[inline]
    fn for_effects(max_sigma: f32, visual: VisualEffectBlurPlan) -> Self {
        if max_sigma <= 0.0 {
            Self::Prepass
        } else if visual.uses_eighth_downsample() {
            Self::Eighth
        } else {
            Self::Quarter
        }
    }

    #[inline]
    fn uses_blur_chain(self) -> bool {
        self != Self::Prepass
    }
}

fn ensure_effect_texture(device: &Device, format: MTLPixelFormat, target: &mut Option<Texture>, size: (u64, u64), creates: &mut u32) {
    let compatible = target
        .as_ref()
        .is_some_and(|texture| texture.width() == size.0 && texture.height() == size.1);
    if compatible {
        return;
    }
    let descriptor = TextureDescriptor::new();
    descriptor.set_pixel_format(format);
    descriptor.set_texture_type(MTLTextureType::D2);
    descriptor.set_width(size.0);
    descriptor.set_height(size.1);
    descriptor.set_storage_mode(MTLStorageMode::Private);
    descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    *target = Some(device.new_texture(&descriptor));
    *creates = creates.saturating_add(1);
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisualEffectBlurPlan {
    sigma_dp: f32,
    downsample_divisor: u64,
    pass_scale: f32,
    pass_sigma: f32,
    pass_radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FinalTargetPlan {
    needs_auxiliary_textures: bool,
    needs_persistent_final_target: bool,
    direct_present: bool,
}

#[inline]
fn final_target_plan(
    sample_count: u32,
    damage_requested: bool,
    frame_color_initialized: bool,
    needs_auxiliary_textures: bool,
    has_compatible_present_texture: bool,
) -> FinalTargetPlan {
    let needs_persistent_final_target = sample_count > 1
        || damage_requested
        || frame_color_initialized
        || !has_compatible_present_texture;
    FinalTargetPlan {
        needs_auxiliary_textures,
        needs_persistent_final_target,
        direct_present: !needs_persistent_final_target,
    }
}

impl VisualEffectBlurPlan {
    const OFF: Self = Self {
        sigma_dp: 0.0,
        downsample_divisor: 1,
        pass_scale: 1.0,
        pass_sigma: 0.0,
        pass_radius: 0.0,
    };

    #[inline]
    fn uses_eighth_downsample(self) -> bool {
        self.downsample_divisor >= 8
    }
}

fn visual_effect_blur_plan(effect: api::VisualEffect) -> VisualEffectBlurPlan {
    let intensity = effect.blur_intensity();
    if intensity <= 0.0 {
        return VisualEffectBlurPlan::OFF;
    }

    let downsample_divisor = if intensity < 0.75 { 4 } else { 8 };
    let pass_scale = downsample_divisor as f32;
    let sigma_dp = DARK_POPUP_MAX_BLUR_SIGMA_DP * intensity;
    let pass_sigma = (sigma_dp / pass_scale).max(0.001);
    let pass_radius = (pass_sigma * 3.0).ceil().clamp(2.0, 192.0);

    VisualEffectBlurPlan { sigma_dp, downsample_divisor, pass_scale, pass_sigma, pass_radius }
}

fn metal_effect_graph_region(
    rect: api::RectF,
    sigma: f32,
    width: u32,
    height: u32,
    scale: f32,
) -> api::EffectGraphRegion {
    let scale = scale.max(1.0);
    let margin = (3.0 * sigma.max(0.0)).ceil();
    let x0 = ((rect.x - margin) * scale).floor().max(0.0).min(width as f32) as u32;
    let y0 = ((rect.y - margin) * scale).floor().max(0.0).min(height as f32) as u32;
    let x1 = ((rect.x + rect.w + margin) * scale).ceil().max(0.0).min(width as f32) as u32;
    let y1 = ((rect.y + rect.h + margin) * scale).ceil().max(0.0).min(height as f32) as u32;
    api::EffectGraphRegion::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

fn metal_effect_graph_output_region(
    rect: api::RectF,
    width: u32,
    height: u32,
    scale: f32,
) -> api::EffectGraphRegion {
    let scale = scale.max(1.0);
    let x0 = (rect.x * scale).floor().max(0.0).min(width as f32) as u32;
    let y0 = (rect.y * scale).floor().max(0.0).min(height as f32) as u32;
    let x1 = ((rect.x + rect.w) * scale).ceil().max(0.0).min(width as f32) as u32;
    let y1 = ((rect.y + rect.h) * scale).ceil().max(0.0).min(height as f32) as u32;
    api::EffectGraphRegion::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

#[inline]
fn effect_graph_hash_mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x100000001b3);
}

fn build_metal_effect_graph(
    events: &mut alloc::vec::Vec<api::EffectGraphEvent>,
    plan: &mut api::EffectGraphPlan,
    list: DrawListView<'_>,
    width: u32,
    height: u32,
    scale: f32,
    sample_count: u32,
    bytes_per_pixel: u8,
) {
    events.clear();
    let target = api::EffectGraphTarget {
        id: 0,
        format: 0,
        sample_count: sample_count.min(u32::from(u8::MAX)) as u8,
        bytes_per_pixel,
        storage: api::EffectGraphStorage::Transient,
    };
    let mut effect_seen = false;
    let mut barrier_pending = false;
    for (command, item) in list.items.iter().enumerate() {
        let effect = match item {
            api::DrawCmd::Backdrop { rect, sigma, .. } => Some((
                *rect,
                sigma.max(0.0),
                api::EffectGraphPyramidSpec {
                    sigma_bits: sigma.max(0.0).to_bits(),
                    quality: 1,
                    downsample_levels: 2,
                    blur_passes: 2,
                    materialized: *sigma > 0.0,
                },
            )),
            api::DrawCmd::VisualEffect { rect, effect } => {
                let blur = visual_effect_blur_plan(*effect);
                Some((
                    *rect,
                    blur.sigma_dp,
                    api::EffectGraphPyramidSpec {
                        sigma_bits: blur.sigma_dp.to_bits(),
                        quality: 2,
                        downsample_levels: if blur.uses_eighth_downsample() { 3 } else { 2 },
                        blur_passes: 2,
                        materialized: blur.sigma_dp > 0.0,
                    },
                ))
            }
            _ => None,
        };
        let Some((rect, sigma, pyramid)) = effect else {
            barrier_pending |= effect_seen;
            continue;
        };
        if barrier_pending {
            events.push(api::EffectGraphEvent {
                command: command as u32,
                target,
                kind: api::EffectGraphEventKind::Write,
            });
            barrier_pending = false;
        }
        effect_seen = true;
        let kind = {
            let region = metal_effect_graph_region(rect, sigma, width, height, scale);
            api::EffectGraphEventKind::Effect {
                source: region,
                destination: region,
                output: metal_effect_graph_output_region(rect, width, height, scale),
                pyramid,
            }
        };
        events.push(api::EffectGraphEvent { command: command as u32, target, kind });
    }
    plan.build(events);
}

#[derive(Clone, Copy)]
struct DrawListView<'a> {
    items: &'a [api::DrawCmd],
    vertices: &'a [api::Vertex],
    indices: &'a [u16],
}

impl<'a> DrawListView<'a> {
    fn from_draw_list(list: &'a api::DrawList) -> Self {
        Self { items: &list.items, vertices: &list.vertices, indices: &list.indices }
    }
}

#[derive(Default)]
struct FilteredDrawList {
    items: alloc::vec::Vec<api::DrawCmd>,
}

impl FilteredDrawList {
    fn view<'a>(&'a self, source: &'a api::DrawList) -> DrawListView<'a> {
        DrawListView { items: &self.items, vertices: &source.vertices, indices: &source.indices }
    }
}

fn vertex_span_rect(vertices: &[api::Vertex], span: api::VertexSpan) -> Option<api::RectF> {
    let start = span.offset as usize;
    let end = start.checked_add(span.len as usize)?;
    let src = vertices.get(start..end)?;
    if src.is_empty() {
        return None;
    }
    let mut minx = f32::INFINITY;
    let mut miny = f32::INFINITY;
    let mut maxx = f32::NEG_INFINITY;
    let mut maxy = f32::NEG_INFINITY;
    for v in src.iter() {
        minx = minx.min(v.x);
        miny = miny.min(v.y);
        maxx = maxx.max(v.x);
        maxy = maxy.max(v.y);
    }
    Some(api::RectF { x: minx, y: miny, w: (maxx - minx).max(0.0), h: (maxy - miny).max(0.0) })
}

fn filter_drawlist_by_dp_scissor(list: &api::DrawList, sc: api::RectI, out: &mut FilteredDrawList) {
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
    out.items.clear();
    if out.items.capacity() < list.items.len() {
        out.items.reserve(list.items.len() - out.items.capacity());
    }
    let mut i = 0usize;
    while i < list.items.len() {
        match &list.items[i] {
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
            api::DrawCmd::Backdrop { rect, .. }
            | api::DrawCmd::VisualEffect { rect, .. }
            | api::DrawCmd::RRect { rect, .. }
            | api::DrawCmd::CameraBg { rect, .. }
            | api::DrawCmd::NineSlice { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::GlyphRun { run } => {
                if let Some(rect) = vertex_span_rect(&list.vertices, run.vb) {
                    if rect_intersects(&rect, &sc) {
                        out.items.push(list.items[i].clone());
                    }
                }
                i += 1;
            }
            api::DrawCmd::ImageMesh { vb, .. } => {
                if let Some(rect) = vertex_span_rect(&list.vertices, *vb) {
                    if rect_intersects(&rect, &sc) {
                        out.items.push(list.items[i].clone());
                    }
                }
                i += 1;
            }
            api::DrawCmd::LayerBegin { rect, .. } => {
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
            api::DrawCmd::Solid { vb, .. } => {
                if let Some(rect) = vertex_span_rect(&list.vertices, *vb) {
                    if rect_intersects(&rect, &sc) {
                        out.items.push(list.items[i].clone());
                    }
                }
                i += 1;
            }
            api::DrawCmd::LayerEnd | api::DrawCmd::ClipPush { .. } | api::DrawCmd::ClipPop => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
        }
    }
}

fn damage_prefiltered_drawlist_view<'a>(
    list: &'a api::DrawList,
    scissor: Option<api::RectI>,
    damage_pct: f32,
    prefilter_thresh: f32,
    filtered: &'a mut FilteredDrawList,
) -> (DrawListView<'a>, usize, bool) {
    let Some(scissor) = scissor else {
        return (DrawListView::from_draw_list(list), 0, false);
    };
    if damage_pct > prefilter_thresh {
        return (DrawListView::from_draw_list(list), 0, false);
    }
    filter_drawlist_by_dp_scissor(list, scissor, filtered);
    (filtered.view(list), list.items.len().saturating_sub(filtered.items.len()), true)
}

fn clear_layer_sublist(sub: &mut api::DrawList, item_capacity: usize) {
    sub.items.clear();
    sub.vertices.clear();
    sub.indices.clear();
    if sub.items.capacity() < item_capacity {
        sub.items.reserve(item_capacity - sub.items.capacity());
    }
}

fn build_layer_sublist(
    list: &api::DrawList,
    begin: usize,
    end: usize,
    origin: [f32; 2],
    sub: &mut api::DrawList,
) {
    clear_layer_sublist(sub, end.saturating_sub(begin + 1));
    let ox = origin[0];
    let oy = origin[1];
    for command in &list.items[begin + 1..end] {
        match command {
            api::DrawCmd::LayerBegin { id, rect, dirty } => {
                sub.items.push(api::DrawCmd::LayerBegin {
                    id: *id,
                    rect: api::RectF::new(rect.x - ox, rect.y - oy, rect.w, rect.h),
                    dirty: *dirty,
                });
            }
            api::DrawCmd::LayerEnd => sub.items.push(api::DrawCmd::LayerEnd),
            api::DrawCmd::ClipPush { rect } => {
                let mut rect = *rect;
                rect.x -= ox as i32;
                rect.y -= oy as i32;
                sub.items.push(api::DrawCmd::ClipPush { rect });
            }
            api::DrawCmd::ClipPop => sub.items.push(api::DrawCmd::ClipPop),
            api::DrawCmd::RRect { rect, radii, color } => {
                sub.items.push(api::DrawCmd::RRect {
                    rect: api::RectF::new(rect.x - ox, rect.y - oy, rect.w, rect.h),
                    radii: *radii,
                    color: *color,
                });
            }
            api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                sub.items.push(api::DrawCmd::NineSlice {
                    tex: *tex,
                    rect: api::RectF::new(rect.x - ox, rect.y - oy, rect.w, rect.h),
                    slice: *slice,
                    alpha: *alpha,
                });
            }
            api::DrawCmd::Image { tex, dst, src, alpha } => {
                sub.items.push(api::DrawCmd::Image {
                    tex: *tex,
                    dst: api::RectF::new(dst.x - ox, dst.y - oy, dst.w, dst.h),
                    src: *src,
                    alpha: *alpha,
                });
            }
            api::DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
                if let Some((vb, ib)) = append_offset_geometry_to_sublist(
                    &list.vertices,
                    &list.indices,
                    sub,
                    *vb,
                    *ib,
                    ox,
                    oy,
                ) {
                    sub.items.push(api::DrawCmd::ImageMesh { tex: *tex, vb, ib, alpha: *alpha });
                }
            }
            api::DrawCmd::Spinner { center, atom, alpha } => {
                sub.items.push(api::DrawCmd::Spinner {
                    center: [center[0] - ox, center[1] - oy],
                    atom: *atom,
                    alpha: *alpha,
                });
            }
            api::DrawCmd::GlyphRun { run } => {
                if let Some((vb, ib)) = append_offset_geometry_to_sublist(
                    &list.vertices,
                    &list.indices,
                    sub,
                    run.vb,
                    run.ib,
                    ox,
                    oy,
                ) {
                    sub.items.push(api::DrawCmd::GlyphRun {
                        run: api::GlyphRun {
                            atlas: run.atlas,
                            atlas_revision: run.atlas_revision,
                            vb,
                            ib,
                            sdf: run.sdf,
                            color: run.color,
                        },
                    });
                }
            }
            api::DrawCmd::CameraBg {
                rect,
                tint,
                alpha,
                grayscale,
                blur,
                sigma,
            } => {
                sub.items.push(api::DrawCmd::CameraBg {
                    rect: api::RectF::new(rect.x - ox, rect.y - oy, rect.w, rect.h),
                    tint: *tint,
                    alpha: *alpha,
                    grayscale: *grayscale,
                    blur: *blur,
                    sigma: *sigma,
                });
            }
            api::DrawCmd::Solid { .. }
            | api::DrawCmd::Backdrop { .. }
            | api::DrawCmd::VisualEffect { .. } => {}
        }
    }
}

impl MetalRenderer {
    fn ensure_image_argument_capacity(&mut self, slot: usize, needed: usize)
    {
        if !self.use_image_arg_buffer || self.img_arg_stride == 0
        {
            return;
        }
        if let Some(buffers) = self.img_arg_bufs.as_mut()
        {
            if buffers.ensure_capacity(&self.device, slot, needed)
            {
                self.acc_resource_grows = self.acc_resource_grows.saturating_add(1);
                self.acc_image_argument_buffer_grows =
                    self.acc_image_argument_buffer_grows.saturating_add(1);
            }
        }
    }

    fn prepare_image_argument_buffers(&mut self, slot: usize)
    {
        let retained_high_water = self
            .image_arg_tables
            .len()
            .max(1)
            .saturating_mul(self.img_arg_stride);
        self.ensure_image_argument_capacity(slot, retained_high_water);
    }

    fn build_layer_plans(&mut self, list: &api::DrawList) {
        self.layer_plans.clear();
        self.layer_plan_stack.clear();
        if !self.layer_cache_enabled {
            return;
        }
        for (index, command) in list.items.iter().enumerate() {
            if !self.layer_plan_stack.is_empty() {
                self.acc_layer_body_commands_scanned =
                    self.acc_layer_body_commands_scanned.saturating_add(1);
            }
            match command {
                api::DrawCmd::LayerBegin { id, rect, dirty } => {
                    self.layer_plan_stack.push(LayerPlanStackEntry {
                        id: *id,
                        begin: index,
                        rect: *rect,
                        dirty: *dirty,
                        unsupported: false,
                    });
                }
                api::DrawCmd::LayerEnd => {
                    let Some(mut pending) = self.layer_plan_stack.pop() else { continue };
                    let w = (pending.rect.w * self.target_scale.max(1.0)).ceil() as u32;
                    let h = (pending.rect.h * self.target_scale.max(1.0)).ceil() as u32;
                    pending.unsupported |= w == 0
                        || h == 0
                        || !pending.rect.x.is_finite()
                        || !pending.rect.y.is_finite()
                        || !pending.rect.w.is_finite()
                        || !pending.rect.h.is_finite();
                    let existing = self.layers.get(&pending.id);
                    let valid_size = existing
                        .map(|entry| {
                            entry.w == w
                                && entry.h == h
                                && entry.tex.pixel_format() == self.color_format
                                && entry.prepared_key.is_none()
                        })
                        .unwrap_or(false);
                    let refresh = !pending.unsupported && (pending.dirty || !valid_size);
                    let generation = existing.map_or(1, |entry| {
                        if refresh { entry.generation.wrapping_add(1) } else { entry.generation }
                    });
                    let action = if pending.unsupported {
                        LayerPlanAction::Inline
                    } else {
                        LayerPlanAction::Composite
                    };
                    if pending.unsupported {
                        if let Some(parent) = self.layer_plan_stack.last_mut() {
                            parent.unsupported = true;
                        }
                    } else if refresh {
                        if let Some(parent) = self.layer_plan_stack.last_mut() {
                            parent.dirty = true;
                        }
                        self.acc_backend_cache_misses = self.acc_backend_cache_misses.saturating_add(1);
                        self.acc_layer_cache_misses = self.acc_layer_cache_misses.saturating_add(1);
                    } else {
                        self.acc_backend_cache_hits = self.acc_backend_cache_hits.saturating_add(1);
                        self.acc_layer_cache_hits = self.acc_layer_cache_hits.saturating_add(1);
                    }
                    self.layer_plans.push(LayerPlan {
                        id: pending.id,
                        begin: pending.begin,
                        end: index,
                        rect: pending.rect,
                        generation,
                        refresh,
                        action,
                    });
                }
                api::DrawCmd::Solid { .. }
                | api::DrawCmd::Backdrop { .. }
                | api::DrawCmd::VisualEffect { .. } => {
                    for pending in &mut self.layer_plan_stack {
                        pending.unsupported = true;
                    }
                }
                _ => {}
            }
        }
        self.layer_plan_stack.clear();
        self.layer_frame_ids.clear();
        let mut required_bytes = 0_u64;
        for plan in &self.layer_plans
        {
            if plan.action != LayerPlanAction::Composite || !self.layer_frame_ids.insert(plan.id)
            {
                continue;
            }
            let width = (plan.rect.w * self.target_scale.max(1.0)).ceil() as u32;
            let height = (plan.rect.h * self.target_scale.max(1.0)).ceil() as u32;
            let descriptor = Self::layer_texture_descriptor(self.color_format, width, height);
            required_bytes = required_bytes.saturating_add(
                self.device.heap_texture_size_and_align(&descriptor).size as u64,
            );
        }
        if required_bytes > self.layer_cache_budget_bytes
        {
            for plan in &mut self.layer_plans
            {
                plan.action = LayerPlanAction::Inline;
                plan.refresh = false;
            }
            self.layer_frame_ids.clear();
        }
    }

    fn layer_plan(&self, id: u32, begin: usize) -> Option<LayerPlan> {
        self.layer_plans
            .iter()
            .copied()
            .find(|plan| plan.id == id && plan.begin == begin)
            .or_else(|| self.layer_plans.iter().copied().find(|plan| plan.id == id))
    }
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
        self.layer_frame_ids.clear();
        self.id_mask_frame_cache_serials.clear();
        if self.id_mask_cache_resident_bytes > self.id_mask_cache_budget_bytes
        {
            self.enforce_id_mask_cache_budget();
        }
        let frame_resource_depth = self.frames.len();
        let preferred_slot = self.next_frame_slot();
        let busy_slots = self.frame_in_flight.load(Ordering::Acquire);
        self.clear_completed_id_mask_generations(busy_slots);
        let mut candidate = preferred_slot;
        let mut slot = None;
        for _ in 0..frame_resource_depth
        {
            if busy_slots & frame_slot_bit(candidate) == 0
            {
                slot = Some(candidate);
                break;
            }
            candidate += 1;
            if candidate == frame_resource_depth
            {
                candidate = 0;
            }
        }
        self.frame_backpressure_skipped = slot.is_none();
        self.frame_slot = slot.unwrap_or(preferred_slot);
        if let Some(slot) = slot {
            self.frames[slot].prepare_for_encode();
        }
        self.acc_draws = 0;
        self.acc_flat_instanced_draws = 0;
        self.acc_instanced = 0;
        self.acc_analytic_instance_bytes = 0;
        self.acc_analytic_instance_buffer_binds = 0;
        self.acc_analytic_instance_ring_grows = 0;
        self.acc_scene3d_draws = 0;
        self.acc_scene3d_instances = 0;
        self.acc_scene3d_instance_bytes = 0;
        self.acc_scene3d_pipeline_binds = 0;
        self.acc_scene3d_depth_state_binds = 0;
        self.acc_scene3d_cull_sets = 0;
        self.acc_scene3d_mesh_buffer_binds = 0;
        self.acc_scene3d_instance_buffer_binds = 0;
        self.acc_scene3d_instance_ring_grows = 0;
        self.acc_scene3d_viewport_sets = 0;
        self.acc_glyph_instance_bytes = 0;
        self.acc_glyph_instance_buffer_binds = 0;
        self.acc_glyph_instances = 0;
        self.acc_icb_cmds = 0;
        self.img_arg_used = 0;
        self.image_arg_table_index.clear();
        self.image_arg_table_count = 0;
        if self.accounting_stats_enabled {
            self.acc_commands_traversed = 0;
            self.acc_commands_copied = 0;
            self.acc_geometry_bytes_copied = 0;
            self.acc_chunks_reused = 0;
            self.acc_chunks_rebuilt = 0;
            self.acc_chunks_prepared = 0;
            self.acc_prepared_plan_reuses = 0;
            self.acc_backend_cache_hits = 0;
            self.acc_backend_cache_misses = 0;
            self.acc_damage_instances_visited = 0;
            self.acc_damage_instances_matched = 0;
            self.acc_damage_commands_visited = 0;
            self.acc_damage_commands_matched = 0;
            self.acc_damage_vertices_visited = 0;
            self.acc_damage_query_ns = 0;
            self.acc_layer_body_commands_scanned = 0;
            self.acc_layer_body_commands_copied = 0;
            self.acc_layer_texture_creates = 0;
            self.acc_layer_cache_hits = 0;
            self.acc_layer_cache_misses = 0;
            self.acc_layer_offscreen_draws = 0;
            self.acc_layer_inline_draws = 0;
            self.acc_layer_double_render_prevented = 0;
            self.acc_id_mask_cache_hits = 0;
            self.acc_id_mask_cache_misses = 0;
            self.acc_id_mask_raster_passes = 0;
            self.acc_id_mask_field_seed_passes = 0;
            self.acc_id_mask_field_jump_passes = 0;
            self.acc_id_mask_compositor_passes = 0;
            self.acc_id_mask_target_creates = 0;
            self.acc_image_argument_encodes = 0;
            self.acc_image_argument_binds = 0;
            self.acc_image_argument_tables_finalized = 0;
            self.acc_image_argument_table_reuses = 0;
            self.acc_image_argument_bytes = 0;
            self.acc_image_argument_buffer_grows = 0;
            self.acc_render_passes = 0;
            self.acc_blit_passes = 0;
            self.acc_texture_copies = 0;
            self.acc_texture_copy_pixels = 0;
            self.acc_texture_copy_bytes = 0;
            self.acc_resource_creates = 0;
            self.acc_resource_grows = 0;
        }
        self.acc_damage_forced_full_refreshes = 0;
        self.acc_culled = 0;
        // Defer command buffer creation until either encode_scene3d or encode_pass.
        if !self.frame_backpressure_skipped {
            self.frames[self.frame_slot].cmd = None;
        }
        self.frame_2d_encoded = false;
        self.frame_present_direct_to_drawable = false;
        self.frame_color_initialized = false;
        self.frame_depth_initialized = false;
        self.frame_gpu_trace = None;
        self.frame_encode_started_at = Some(Instant::now());
        let memory = self.last_stats.memory;
        self.last_stats = PerfStats {
            memory,
            frame_backpressure_skipped: self.frame_backpressure_skipped as u32,
            skipped_submissions: self.frame_backpressure_skipped as u32,
            ..PerfStats::default()
        };
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
        if self.accounting_stats_enabled {
            self.acc_commands_traversed = self
                .acc_commands_traversed
                .saturating_add(list.items.len() as u64);
        }
        if self.frame_backpressure_skipped {
            return;
        }
        if self.submit_error_flag.load(Ordering::Acquire) {
            if ios_log_enabled() {
                ios_log("oxide.renderer-metal: skipping encode_pass due pending submit error");
            }
            return;
        }
        let slot = self.current_frame_slot();
        self.prepare_image_argument_buffers(slot);
        let cmd = self.ensure_frame_command_buffer(slot);

        // Adaptive policy: compute camera coverage and environment (iOS thermal/LPM),
        // then tune blur update period and optionally pause camera when hot with tiny coverage.
        let vp_w_dp = (self.target_w as f32) / self.target_scale.max(1.0);
        let vp_h_dp = (self.target_h as f32) / self.target_scale.max(1.0);
        let vp_area_dp = (vp_w_dp.max(1.0)) * (vp_h_dp.max(1.0));
        let mut cam_area: f32 = 0.0;
        let mut need_cam_blur = false;
        let mut requested_cam_blur_sigma = 0.0f32;
        let mut has_backdrop = false;
        let mut has_visual_effect = false;
        let mut has_layer_commands = false;
        let mut visual_effect_plan = VisualEffectBlurPlan::OFF;
        let effect_bytes_per_pixel = match self.color_format {
            MTLPixelFormat::RGBA16Float | MTLPixelFormat::BGRA10_XR => 8,
            _ => 4,
        };
        let mut effect_graph_key = 0xcbf29ce484222325_u64;
        for value in [
            u64::from(self.target_w),
            u64::from(self.target_h),
            u64::from(self.target_scale.to_bits()),
            u64::from(self.sample_count),
            u64::from(effect_bytes_per_pixel),
        ] {
            effect_graph_hash_mix(&mut effect_graph_key, value);
        }
        let mut effect_seen = false;
        let mut barrier_pending = false;
        for (command, it) in list.items.iter().enumerate() {
            let effect = match it {
                api::DrawCmd::Backdrop { rect, sigma, .. } => {
                    Some((1_u64, *rect, sigma.max(0.0)))
                }
                api::DrawCmd::VisualEffect { rect, effect } => {
                    Some((2, *rect, visual_effect_blur_plan(*effect).sigma_dp))
                }
                _ => None,
            };
            if let Some((kind, rect, sigma)) = effect {
                if barrier_pending {
                    effect_graph_hash_mix(&mut effect_graph_key, 0xff);
                    barrier_pending = false;
                }
                effect_seen = true;
                let signature = kind.wrapping_mul(0x9e3779b185ebca87)
                    ^ (command as u64).wrapping_mul(0xc2b2ae3d27d4eb4f)
                    ^ u64::from(rect.x.to_bits()).rotate_left(7)
                    ^ u64::from(rect.y.to_bits()).rotate_left(19)
                    ^ u64::from(rect.w.to_bits()).rotate_left(31)
                    ^ u64::from(rect.h.to_bits()).rotate_left(43)
                    ^ u64::from(sigma.to_bits()).rotate_left(53);
                effect_graph_hash_mix(&mut effect_graph_key, signature);
            } else {
                barrier_pending |= effect_seen;
            }
            match it {
                api::DrawCmd::CameraBg { rect, blur, sigma, .. } => {
                    let a = (rect.w.max(0.0) * rect.h.max(0.0)).min(vp_area_dp);
                    cam_area += a;
                    if *blur {
                        need_cam_blur = true;
                        requested_cam_blur_sigma = requested_cam_blur_sigma.max(*sigma);
                    }
                }
                api::DrawCmd::Backdrop { .. } => {
                    has_backdrop = true;
                }
                api::DrawCmd::VisualEffect { effect, .. } => {
                    has_backdrop = true;
                    has_visual_effect = true;
                    let plan = visual_effect_blur_plan(*effect);
                    if plan.sigma_dp > visual_effect_plan.sigma_dp {
                        visual_effect_plan = plan;
                    }
                }
                api::DrawCmd::LayerBegin { .. } | api::DrawCmd::LayerEnd => {
                    has_layer_commands = true;
                }
                _ => {}
            }
        }
        effect_graph_key = if effect_seen { effect_graph_key.max(1) } else { 0 };
        let mut effect_graph_events = core::mem::take(&mut self.effect_graph_events);
        let mut effect_graph_plan = core::mem::take(&mut self.effect_graph_plan);
        let effect_graph_reused = effect_graph_key != 0 && effect_graph_key == self.effect_graph_key;
        if effect_graph_key != 0 && !effect_graph_reused {
            build_metal_effect_graph(
                &mut effect_graph_events,
                &mut effect_graph_plan,
                DrawListView::from_draw_list(list),
                self.target_w,
                self.target_h,
                self.target_scale,
                self.sample_count,
                effect_bytes_per_pixel,
            );
        } else if effect_graph_key == 0 && self.effect_graph_key != 0 {
            effect_graph_events.clear();
            effect_graph_plan.build(&effect_graph_events);
        }
        self.effect_graph_key = effect_graph_key;
        let effect_graph_stats = effect_graph_plan.stats();
        let max_effect_sigma = effect_graph_plan.pyramids().iter().fold(0.0f32, |sigma, pyramid| {
            sigma.max(f32::from_bits(pyramid.spec.sigma_bits))
        });
        let effect_capture_region = effect_graph_plan.captures().iter().fold(
            api::EffectGraphRegion::default(),
            |region, capture| region.union(capture.destination),
        );
        self.effect_graph_events = effect_graph_events;
        self.effect_graph_plan = effect_graph_plan;
        self.last_stats.effect_graph_effects = effect_graph_stats.effects;
        self.last_stats.effect_graph_captures = effect_graph_stats.captures;
        self.last_stats.effect_graph_pyramids = effect_graph_stats.pyramids;
        self.last_stats.effect_graph_pyramid_reuses = effect_graph_stats.pyramid_reuses;
        self.last_stats.effect_graph_plan_reuses = effect_graph_reused as u32;
        self.last_stats.effect_graph_capture_passes = effect_graph_stats.capture_passes;
        self.last_stats.effect_graph_downsample_passes = effect_graph_stats.downsample_passes;
        self.last_stats.effect_graph_blur_horizontal_passes =
            effect_graph_stats.blur_horizontal_passes;
        self.last_stats.effect_graph_blur_vertical_passes =
            effect_graph_stats.blur_vertical_passes;
        self.last_stats.effect_graph_composite_passes = effect_graph_stats.composite_passes;
        self.last_stats.effect_graph_max_lifetime_commands =
            effect_graph_stats.max_lifetime_commands;
        self.last_stats.effect_graph_resources = effect_graph_stats.resources;
        self.last_stats.effect_graph_alias_slots = effect_graph_stats.alias_slots;
        self.last_stats.effect_graph_logical_bytes = effect_graph_stats.logical_bytes;
        self.last_stats.effect_graph_physical_bytes = effect_graph_stats.physical_bytes;
        self.last_stats.effect_graph_aliased_bytes = effect_graph_stats.aliased_bytes;
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
                let (cam_blur_passes, cam_blur_pass_sigma) =
                    camera_blur_pass_plan(requested_cam_blur_sigma);
                let blur_t0 = std::time::Instant::now();
                let now = std::time::Instant::now();
                let vp_dp: [f32; 2] = [
                    (self.target_w as f32) / self.target_scale.max(1.0),
                    (self.target_h as f32) / self.target_scale.max(1.0),
                ];
                let rect_dp: [f32; 4] = [0.0, 0.0, vp_dp[0], vp_dp[1]];
                self.ensure_effect_targets(EffectTargetPlan::Quarter);
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
                    self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
                    self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
                    self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
                    for _ in 0..cam_blur_passes {
                        let rpd = RenderPassDescriptor::new();
                        let ca = rpd.color_attachments().object_at(0).unwrap();
                        ca.set_texture(Some(qtmp));
                        ca.set_load_action(MTLLoadAction::DontCare);
                        ca.set_store_action(MTLStoreAction::Store);
                        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
                        let enc = cmd.new_render_command_encoder(&rpd);
                        if let Some(sam) = &self.sampler {
                            enc.set_fragment_sampler_state(0, Some(sam));
                        }
                        enc.set_fragment_texture(0, Some(q));
                        let params_h: [f32; 4] = [1.0, 0.0, cam_blur_pass_sigma, 0.0];
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
                        let kernel = bind_blur_kernel(
                            &enc,
                            params_h,
                            self.paired_blur_allowed(),
                            &self.pso_blur,
                            &self.pso_blur_paired,
                        );
                        record_blur_kernel_use(&mut self.last_stats, kernel);
                        enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                        enc.end_encoding();
                        let rpd2 = RenderPassDescriptor::new();
                        let ca2 = rpd2.color_attachments().object_at(0).unwrap();
                        ca2.set_texture(Some(q));
                        ca2.set_load_action(MTLLoadAction::DontCare);
                        ca2.set_store_action(MTLStoreAction::Store);
                        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
                        let enc2 = cmd.new_render_command_encoder(&rpd2);
                        if let Some(sam) = &self.sampler {
                            enc2.set_fragment_sampler_state(0, Some(sam));
                        }
                        enc2.set_fragment_texture(0, Some(qtmp));
                        let params_v: [f32; 4] = [0.0, 1.0, cam_blur_pass_sigma, 0.0];
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
                        let kernel = bind_blur_kernel(
                            &enc2,
                            params_v,
                            self.paired_blur_allowed(),
                            &self.pso_blur,
                            &self.pso_blur_paired,
                        );
                        record_blur_kernel_use(&mut self.last_stats, kernel);
                        enc2.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                        enc2.end_encoding();
                    }
                    self.cam_blur_tex = Some(q.to_owned());
                }
                self.cam_last_update = Some(std::time::Instant::now());
                blur_ms_out = blur_t0.elapsed().as_secs_f64() * 1000.0;
                blur_updated = 1;
            }
        }

        self.build_layer_plans(list);
        for plan_index in 0..self.layer_plans.len() {
            let plan = self.layer_plans[plan_index];
            if !plan.refresh {
                continue;
            }
            let mut sub = core::mem::take(&mut self.layer_sublist);
            build_layer_sublist(list, plan.begin, plan.end, [plan.rect.x, plan.rect.y], &mut sub);
            self.acc_layer_body_commands_copied = self
                .acc_layer_body_commands_copied
                .saturating_add(sub.items.len() as u64);
            self.acc_commands_copied =
                self.acc_commands_copied.saturating_add(sub.items.len() as u64);
            self.acc_geometry_bytes_copied = self.acc_geometry_bytes_copied.saturating_add(
                (sub.vertices.len() as u64)
                    .saturating_mul(core::mem::size_of::<api::Vertex>() as u64)
                    .saturating_add(
                        (sub.indices.len() as u64)
                            .saturating_mul(core::mem::size_of::<u16>() as u64),
                    ),
            );
            let w = (plan.rect.w * self.target_scale.max(1.0)).ceil() as u32;
            let h = (plan.rect.h * self.target_scale.max(1.0)).ceil() as u32;
            let existing_texture = self
                .layers
                .get(&plan.id)
                .filter(|entry| {
                    entry.w == w
                        && entry.h == h
                        && entry.tex.pixel_format() == self.color_format
                })
                .map(|entry| (entry.tex.to_owned(), entry.bytes));
            let (texture, texture_bytes) = if let Some(texture) = existing_texture
            {
                texture
            }
            else
            {
                if let Some(entry) = self.layers.remove(&plan.id)
                {
                    self.layer_cache_recreations = self.layer_cache_recreations.saturating_add(1);
                    self.recycle_layer_entry(entry);
                }
                let Some(texture) = self.acquire_layer_texture(self.color_format, w, h) else
                {
                    self.layer_plans[plan_index].action = LayerPlanAction::Inline;
                    self.layer_plans[plan_index].refresh = false;
                    continue;
                };
                texture
            };
            let pass_descriptor = RenderPassDescriptor::new();
            let attachment = pass_descriptor.color_attachments().object_at(0).unwrap();
            attachment.set_texture(Some(&texture));
            attachment.set_load_action(MTLLoadAction::Clear);
            attachment.set_clear_color(MTLClearColor {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.0,
            });
            attachment.set_store_action(MTLStoreAction::Store);
            self.acc_render_passes = self.acc_render_passes.saturating_add(1);
            let layer_encoder = cmd.new_render_command_encoder(&pass_descriptor);
            let mut layer_frame = self.layer_scratch_frame.take().unwrap_or_else(PerFrame::new);
            layer_frame.vb_used = self.frames[slot].vb_used;
            layer_frame.ib_used = self.frames[slot].ib_used;
            layer_frame.ub_used = self.frames[slot].ub_used;
            layer_frame.cmd = None;
            let old_size = (self.target_w, self.target_h);
            let old_encoding_layer = self.encoding_layer;
            self.target_w = w;
            self.target_h = h;
            self.encoding_layer = true;
            let draws_before = u64::from(self.acc_draws).saturating_add(u64::from(self.acc_instanced));
            encode_draws(
                &layer_encoder,
                &mut layer_frame,
                self,
                DrawListView::from_draw_list(&sub),
                false,
                None,
            );
            let draws_after = u64::from(self.acc_draws).saturating_add(u64::from(self.acc_instanced));
            self.acc_layer_offscreen_draws = self
                .acc_layer_offscreen_draws
                .saturating_add(draws_after.saturating_sub(draws_before));
            self.target_w = old_size.0;
            self.target_h = old_size.1;
            self.encoding_layer = old_encoding_layer;
            layer_encoder.end_encoding();
            self.frames[slot].vb_used = layer_frame.vb_used;
            self.frames[slot].ib_used = layer_frame.ib_used;
            self.frames[slot].ub_used = layer_frame.ub_used;
            self.layer_scratch_frame = Some(layer_frame);
            if let Some(entry) = self
                .layers
                .get_mut(&plan.id)
                .filter(|entry| {
                    entry.w == w
                        && entry.h == h
                        && entry.tex.pixel_format() == self.color_format
                })
            {
                entry.generation = plan.generation;
                entry.prepared_key = None;
                entry.resources.clear();
                entry.last_used_frame = self.frame_id;
            } else {
                self.layers.insert(
                    plan.id,
                    LayerEntry {
                        tex: texture,
                        w,
                        h,
                        generation: plan.generation,
                        prepared_key: None,
                        resources: alloc::vec::Vec::new(),
                        bytes: texture_bytes,
                        last_used_frame: self.frame_id,
                    },
                );
            }
            self.acc_layer_double_render_prevented =
                self.acc_layer_double_render_prevented.saturating_add(1);
            self.layer_sublist = sub;
        }

        // Effects prepass: if there is any Backdrop, render a prepass and blur it.
        if has_backdrop {
            self.ensure_effect_targets(EffectTargetPlan::for_effects(
                max_effect_sigma,
                visual_effect_plan,
            ));
            // 1) Prepass: render up to the first Backdrop into prepass_tex
            let rpd0 = RenderPassDescriptor::new();
            let ca_pre = rpd0.color_attachments().object_at(0).unwrap();
            if let Some(src) = &self.prepass_tex {
                ca_pre.set_texture(Some(src));
            }
            ca_pre.set_load_action(MTLLoadAction::Clear);
            ca_pre.set_clear_color(MTLClearColor { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 });
            ca_pre.set_store_action(MTLStoreAction::Store);
            self.acc_render_passes = self.acc_render_passes.saturating_add(1);
            let enc0 = cmd.new_render_command_encoder(&rpd0);
            // Move out per-frame to avoid double-borrow
            let mut pf0 = core::mem::take(&mut self.frames[slot]);
            // Compute prepass scissor: union of Backdrop rects (expanded) intersect frame scissor if enabled
            let mut prepass_scissor_dp: Option<api::RectI> = None;
            {
                let s = self.target_scale.max(1.0);
                let region = effect_capture_region;
                if !region.is_empty() {
                    let x0 = (region.x as f32 / s).floor() as i32;
                    let y0 = (region.y as f32 / s).floor() as i32;
                    let x1 = (region.x.saturating_add(region.w) as f32 / s).ceil() as i32;
                    let y1 = (region.y.saturating_add(region.h) as f32 / s).ceil() as i32;
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
            let mut filtered_prepass = core::mem::take(&mut self.filtered_prepass);
            let (list_pre_view, culled_prepass, used_filtered_prepass) =
                damage_prefiltered_drawlist_view(
                    list,
                    prepass_scissor_dp,
                    self.frame_damage_pct,
                    self.damage_prefilter_thresh,
                    &mut filtered_prepass,
                );
            if culled_prepass > 0 {
                self.acc_culled = self.acc_culled.saturating_add(culled_prepass as u32);
            }
            encode_draws(&enc0, &mut pf0, self, list_pre_view, true, prepass_scissor_dp);
            if used_filtered_prepass {
                self.acc_commands_copied = self
                    .acc_commands_copied
                    .saturating_add(filtered_prepass.items.len() as u64);
                filtered_prepass.items.clear();
            }
            self.filtered_prepass = filtered_prepass;
            self.frames[slot] = pf0;
            enc0.end_encoding();

            // Determine blur kernel and union scissor in pixel coords for all Backdrop rects
            let sigma = max_effect_sigma;
            let region = effect_capture_region;
            let u_x0 = region.x as i32;
            let u_y0 = region.y as i32;
            let u_x1 = region.x.saturating_add(region.w) as i32;
            let u_y1 = region.y.saturating_add(region.h) as i32;
            // Clamp to framebuffer bounds and ensure non-negative width/height
            let x0c = u_x0.clamp(0, self.target_w as i32);
            let y0c = u_y0.clamp(0, self.target_h as i32);
            let x1c = u_x1.clamp(0, self.target_w as i32);
            let y1c = u_y1.clamp(0, self.target_h as i32);
            let sc_x = x0c.max(0) as u64;
            let sc_y = y0c.max(0) as u64;
            let sc_w = (x1c - x0c).max(0) as u64;
            let sc_h = (y1c - y0c).max(0) as u64;

            if sigma > 0.0 {
                // 2) Downsample: prepass_tex -> half_tex -> quarter_tex
                let sc_half =
                    MTLScissorRect { x: sc_x / 2, y: sc_y / 2, width: sc_w / 2, height: sc_h / 2 };
                let sc_quarter =
                    MTLScissorRect { x: sc_x / 4, y: sc_y / 4, width: sc_w / 4, height: sc_h / 4 };
                let sc_eighth =
                    MTLScissorRect { x: sc_x / 8, y: sc_y / 8, width: sc_w / 8, height: sc_h / 8 };
                let visual_effect_uses_eighth =
                    has_visual_effect && visual_effect_plan.uses_eighth_downsample();

                // prepass -> half
                let rpd_ds1 = RenderPassDescriptor::new();
                let ca_ds1 = rpd_ds1.color_attachments().object_at(0).unwrap();
                if let Some(dst) = &self.half_tex {
                    ca_ds1.set_texture(Some(dst));
                }
                ca_ds1.set_load_action(MTLLoadAction::DontCare);
                ca_ds1.set_store_action(MTLStoreAction::Store);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
                self.prepass_shaded_px = self
                    .prepass_shaded_px
                    .saturating_add(sc_half.width.saturating_mul(sc_half.height));
                enc_ds1.end_encoding();

                // half -> quarter
                let rpd_ds2 = RenderPassDescriptor::new();
                let ca_ds2 = rpd_ds2.color_attachments().object_at(0).unwrap();
                if let Some(dst) = &self.quarter_tex {
                    ca_ds2.set_texture(Some(dst));
                }
                ca_ds2.set_load_action(MTLLoadAction::DontCare);
                ca_ds2.set_store_action(MTLStoreAction::Store);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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

                if visual_effect_uses_eighth {
                    let rpd_ds3 = RenderPassDescriptor::new();
                    let ca_ds3 = rpd_ds3.color_attachments().object_at(0).unwrap();
                    if let Some(dst) = &self.eighth_tex {
                        ca_ds3.set_texture(Some(dst));
                    }
                    ca_ds3.set_load_action(MTLLoadAction::DontCare);
                    ca_ds3.set_store_action(MTLStoreAction::Store);
                    self.acc_render_passes = self.acc_render_passes.saturating_add(1);
                    let enc_ds3 = cmd.new_render_command_encoder(&rpd_ds3);
                    enc_ds3.set_render_pipeline_state(&self.pso_downsample);
                    if let Some(sam) = &self.sampler {
                        enc_ds3.set_fragment_sampler_state(0, Some(sam));
                    }
                    if let Some(src) = &self.quarter_tex {
                        enc_ds3.set_fragment_texture(0, Some(src));
                    }
                    enc_ds3.set_scissor_rect(sc_eighth);
                    enc_ds3.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                    self.prepass_shaded_px = self
                        .prepass_shaded_px
                        .saturating_add(sc_eighth.width.saturating_mul(sc_eighth.height));
                    enc_ds3.end_encoding();
                }

                let effect_scissor = if visual_effect_uses_eighth { sc_eighth } else { sc_quarter };
                let (effect_pass_sigma, effect_pass_radius) = if has_visual_effect {
                    (visual_effect_plan.pass_sigma, visual_effect_plan.pass_radius)
                } else {
                    let pass_scale = 4.0;
                    let pass_sigma = (sigma / pass_scale).max(0.001);
                    (pass_sigma, (pass_sigma * 3.0).ceil().clamp(2.0, 192.0))
                };

                // 3) Blur at the strongest active effect resolution.
                let rpd1 = RenderPassDescriptor::new();
                let ca_blur_h = rpd1.color_attachments().object_at(0).unwrap();
                if visual_effect_uses_eighth {
                    if let Some(tmp) = &self.eighth_tmp_tex {
                        ca_blur_h.set_texture(Some(tmp));
                    }
                } else if let Some(tmp) = &self.quarter_tmp_tex {
                    ca_blur_h.set_texture(Some(tmp));
                }
                ca_blur_h.set_load_action(MTLLoadAction::DontCare);
                ca_blur_h.set_store_action(MTLStoreAction::Store);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
                let enc1 = cmd.new_render_command_encoder(&rpd1);
                if let Some(sam) = &self.sampler {
                    enc1.set_fragment_sampler_state(0, Some(sam));
                }
                if visual_effect_uses_eighth {
                    if let Some(src) = &self.eighth_tex {
                        enc1.set_fragment_texture(0, Some(src));
                    }
                } else if let Some(src) = &self.quarter_tex {
                    enc1.set_fragment_texture(0, Some(src));
                }
                enc1.set_scissor_rect(effect_scissor);
                let params_h: [f32; 4] = [1.0, 0.0, effect_pass_sigma, effect_pass_radius];
                let kernel_h = bind_blur_kernel(
                    &enc1,
                    params_h,
                    self.paired_blur_allowed(),
                    &self.pso_blur,
                    &self.pso_blur_paired,
                );
                enc1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                self.prepass_shaded_px = self
                    .prepass_shaded_px
                    .saturating_add(effect_scissor.width.saturating_mul(effect_scissor.height));
                enc1.end_encoding();

                let rpd2 = RenderPassDescriptor::new();
                let ca_blur_v = rpd2.color_attachments().object_at(0).unwrap();
                if visual_effect_uses_eighth {
                    if let Some(dst) = &self.eighth_tex {
                        ca_blur_v.set_texture(Some(dst));
                    }
                } else if let Some(dst) = &self.quarter_tex {
                    ca_blur_v.set_texture(Some(dst));
                }
                ca_blur_v.set_load_action(MTLLoadAction::DontCare);
                ca_blur_v.set_store_action(MTLStoreAction::Store);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
                let enc2 = cmd.new_render_command_encoder(&rpd2);
                if let Some(sam) = &self.sampler {
                    enc2.set_fragment_sampler_state(0, Some(sam));
                }
                if visual_effect_uses_eighth {
                    if let Some(tmp) = &self.eighth_tmp_tex {
                        enc2.set_fragment_texture(0, Some(tmp));
                    }
                } else if let Some(tmp) = &self.quarter_tmp_tex {
                    enc2.set_fragment_texture(0, Some(tmp));
                }
                enc2.set_scissor_rect(effect_scissor);
                let params_v: [f32; 4] = [0.0, 1.0, effect_pass_sigma, effect_pass_radius];
                let kernel_v = bind_blur_kernel(
                    &enc2,
                    params_v,
                    self.paired_blur_allowed(),
                    &self.pso_blur,
                    &self.pso_blur_paired,
                );
                enc2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                self.prepass_shaded_px = self
                    .prepass_shaded_px
                    .saturating_add(effect_scissor.width.saturating_mul(effect_scissor.height));
                enc2.end_encoding();
                record_blur_kernel_use(&mut self.last_stats, kernel_h);
                record_blur_kernel_use(&mut self.last_stats, kernel_v);

                if visual_effect_uses_eighth {
                    let rpd_us0 = RenderPassDescriptor::new();
                    let ca_us0 = rpd_us0.color_attachments().object_at(0).unwrap();
                    if let Some(dst) = &self.quarter_tex {
                        ca_us0.set_texture(Some(dst));
                    }
                    ca_us0.set_load_action(MTLLoadAction::DontCare);
                    ca_us0.set_store_action(MTLStoreAction::Store);
                    self.acc_render_passes = self.acc_render_passes.saturating_add(1);
                    let enc_us0 = cmd.new_render_command_encoder(&rpd_us0);
                    enc_us0.set_render_pipeline_state(&self.pso_upsample);
                    if let Some(sam) = &self.sampler {
                        enc_us0.set_fragment_sampler_state(0, Some(sam));
                    }
                    if let Some(src) = &self.eighth_tex {
                        enc_us0.set_fragment_texture(0, Some(src));
                    }
                    let scale2: f32 = 2.0;
                    enc_us0.set_fragment_bytes(
                        1,
                        core::mem::size_of_val(&scale2) as u64,
                        &scale2 as *const _ as *const _,
                    );
                    enc_us0.set_scissor_rect(sc_quarter);
                    enc_us0.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                    self.prepass_shaded_px = self
                        .prepass_shaded_px
                        .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
                    enc_us0.end_encoding();
                }

                // 5) Upsample quarter -> half (scale 2)
                let rpd_us1 = RenderPassDescriptor::new();
                let ca_us1 = rpd_us1.color_attachments().object_at(0).unwrap();
                if let Some(dst) = &self.half_tex {
                    ca_us1.set_texture(Some(dst));
                }
                ca_us1.set_load_action(MTLLoadAction::DontCare);
                ca_us1.set_store_action(MTLStoreAction::Store);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
                self.prepass_shaded_px = self
                    .prepass_shaded_px
                    .saturating_add(sc_half.width.saturating_mul(sc_half.height));
                enc_us1.end_encoding();

                // 6) Upsample half -> prepass (scale 2)
                let rpd_us2 = RenderPassDescriptor::new();
                let ca_us2 = rpd_us2.color_attachments().object_at(0).unwrap();
                if let Some(dst) = &self.prepass_tex {
                    ca_us2.set_texture(Some(dst));
                }
                ca_us2.set_load_action(MTLLoadAction::DontCare);
                ca_us2.set_store_action(MTLStoreAction::Store);
                self.acc_render_passes = self.acc_render_passes.saturating_add(1);
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
        }

        // Damage may load only a complete persistent target with matching policy.
        let dmg_thresh: f32 = self.damage_use_thresh;
        let damage_requested = self.sample_count == 1
            && self.damage_enabled
            && self.frame_scissor_dp.is_some()
            && self.frame_damage_pct < dmg_thresh;
        let target_policy = u8::from(has_backdrop)
            | u8::from(has_visual_effect) << 1
            | u8::from(need_cam_blur) << 2
            | u8::from(has_layer_commands) << 3;
        if !self.frame_color_initialized
            && self.persistent_target_valid
            && self.persistent_target_policy != target_policy
        {
            self.persistent_target_valid = false;
        }
        let use_damage = damage_requested && self.persistent_target_valid;
        if damage_requested && !self.persistent_target_valid {
            self.acc_damage_forced_full_refreshes =
                self.acc_damage_forced_full_refreshes.saturating_add(1);
        }
        let pending_present_texture = self.pending_present_texture as *mut MTLTexture;
        let compatible_present_texture = if !pending_present_texture.is_null() {
            let dst = unsafe { TextureRef::from_ptr(pending_present_texture) };
            if dst.width() as u32 == self.target_w
                && dst.height() as u32 == self.target_h
                && dst.pixel_format() == self.color_format
            {
                Some(dst)
            } else {
                None
            }
        } else {
            None
        };
        let needs_auxiliary_textures = has_backdrop
            || has_visual_effect
            || need_cam_blur
            || has_layer_commands;
        let final_target_plan = final_target_plan(
            self.sample_count,
            damage_requested,
            self.frame_color_initialized,
            needs_auxiliary_textures,
            compatible_present_texture.is_some(),
        );
        if final_target_plan.needs_persistent_final_target {
            self.ensure_target();
            if self.target_tex.is_none() {
                return;
            }
        }
        let direct_present_texture = if final_target_plan.direct_present {
            compatible_present_texture
        } else {
            None
        };
        self.frame_present_direct_to_drawable = final_target_plan.direct_present;
        if self.frame_present_direct_to_drawable {
            self.persistent_target_valid = false;
        }
        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        if self.sample_count > 1 {
            if let Some(msaa) = &self.target_msaa_tex {
                ca0.set_texture(Some(msaa));
            }
            if let Some(dst) = &self.target_tex {
                ca0.set_resolve_texture(Some(dst));
            }
            ca0.set_store_action(MTLStoreAction::MultisampleResolve);
        } else if let Some(dst) = direct_present_texture {
            ca0.set_texture(Some(dst));
            ca0.set_store_action(MTLStoreAction::Store);
        } else {
            if let Some(dst) = &self.target_tex {
                ca0.set_texture(Some(dst));
            }
            ca0.set_store_action(MTLStoreAction::Store);
        }
        if self.frame_color_initialized && self.persistent_target_valid {
            ca0.set_load_action(MTLLoadAction::Load);
        } else if use_damage {
            ca0.set_load_action(MTLLoadAction::Load);
        } else {
            ca0.set_load_action(MTLLoadAction::Clear);
        }
        let clear_alpha = if transparent_drawable_clear_enabled() { 0.0 } else { 1.0 };
        ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: clear_alpha });
        let frame_gpu_trace =
            self.gpu_stage_timing.as_ref().and_then(|timing| timing.begin_submission(&self.device));
        if let Some(gpu_trace) = frame_gpu_trace.as_ref() {
            gpu_trace.configure_render_pass(&rpd);
        }
        self.frame_gpu_trace = frame_gpu_trace;
        if self.accounting_stats_enabled {
            self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        }
        let enc = cmd.new_render_command_encoder(&rpd);
        // Move out per-frame to avoid double-borrow on &mut self
        let mut pf = core::mem::take(&mut self.frames[slot]);
        // Optional pre-filtering by frame scissor to reduce CPU work (small damage only)
        let mut filtered_main = core::mem::take(&mut self.filtered_main);
        let main_scissor = if use_damage { self.frame_scissor_dp } else { None };
        let (list_main_view, culled_main, used_filtered_main) = damage_prefiltered_drawlist_view(
            list,
            main_scissor,
            self.frame_damage_pct,
            self.damage_prefilter_thresh,
            &mut filtered_main,
        );
        if culled_main > 0 {
            self.acc_culled = self.acc_culled.saturating_add(culled_main as u32);
        }
        encode_draws(&enc, &mut pf, self, list_main_view, false, main_scissor);
        if used_filtered_main {
            self.acc_commands_copied = self
                .acc_commands_copied
                .saturating_add(filtered_main.items.len() as u64);
            filtered_main.items.clear();
        }
        self.filtered_main = filtered_main;
        self.frames[slot] = pf;
        enc.end_encoding();

        // Snapshot last stats
        self.last_stats.vb_bytes = self.frames[slot].vb_used as u64;
        self.last_stats.ub_bytes = self.frames[slot].ub_used as u64;
        self.last_stats.ib_bytes = self.frames[slot].ib_used as u64;
        self.last_stats.draws = self.acc_draws.saturating_add(self.acc_flat_instanced_draws);
        self.last_stats.instanced = self.acc_instanced;
        self.last_stats.analytic_instance_bytes = self.acc_analytic_instance_bytes;
        self.last_stats.analytic_instance_buffer_binds =
            self.acc_analytic_instance_buffer_binds;
        self.last_stats.analytic_instance_ring_grows =
            self.acc_analytic_instance_ring_grows;
        self.last_stats.scene3d_draws = self.acc_scene3d_draws;
        self.last_stats.scene3d_instances = self.acc_scene3d_instances;
        self.last_stats.scene3d_instance_bytes = self.acc_scene3d_instance_bytes;
        self.last_stats.scene3d_pipeline_binds = self.acc_scene3d_pipeline_binds;
        self.last_stats.scene3d_depth_state_binds = self.acc_scene3d_depth_state_binds;
        self.last_stats.scene3d_cull_sets = self.acc_scene3d_cull_sets;
        self.last_stats.scene3d_mesh_buffer_binds = self.acc_scene3d_mesh_buffer_binds;
        self.last_stats.scene3d_instance_buffer_binds = self.acc_scene3d_instance_buffer_binds;
        self.last_stats.scene3d_instance_ring_grows = self.acc_scene3d_instance_ring_grows;
        self.last_stats.scene3d_viewport_sets = self.acc_scene3d_viewport_sets;
        self.last_stats.glyph_instance_bytes = self.acc_glyph_instance_bytes;
        self.last_stats.glyph_instance_buffer_binds = self.acc_glyph_instance_buffer_binds;
        self.last_stats.glyph_instances = self.acc_glyph_instances;
        self.last_stats.icb_cmds = self.acc_icb_cmds;
        if self.accounting_stats_enabled {
            self.last_stats.commands_traversed = self.acc_commands_traversed;
            self.last_stats.commands_copied = self.acc_commands_copied;
            self.last_stats.geometry_bytes_copied = self.acc_geometry_bytes_copied;
            self.last_stats.chunks_reused = self.acc_chunks_reused;
            self.last_stats.chunks_rebuilt = self.acc_chunks_rebuilt;
            self.last_stats.chunks_prepared = self.acc_chunks_prepared;
            self.last_stats.backend_cache_hits = self.acc_backend_cache_hits;
            self.last_stats.backend_cache_misses = self.acc_backend_cache_misses;
            self.last_stats.layer_body_commands_scanned = self.acc_layer_body_commands_scanned;
            self.last_stats.layer_body_commands_copied = self.acc_layer_body_commands_copied;
            self.last_stats.layer_texture_creates = self.acc_layer_texture_creates;
            self.last_stats.layer_cache_hits = self.acc_layer_cache_hits;
            self.last_stats.layer_cache_misses = self.acc_layer_cache_misses;
            self.last_stats.layer_offscreen_draws = self.acc_layer_offscreen_draws;
            self.last_stats.layer_inline_draws = self.acc_layer_inline_draws;
            self.last_stats.layer_double_render_prevented =
                self.acc_layer_double_render_prevented;
            self.last_stats.image_argument_encodes = self.acc_image_argument_encodes;
            self.last_stats.image_argument_binds = self.acc_image_argument_binds;
            self.last_stats.image_argument_tables_finalized =
                self.acc_image_argument_tables_finalized;
            self.last_stats.image_argument_table_reuses =
                self.acc_image_argument_table_reuses;
            self.last_stats.image_argument_bytes = self.acc_image_argument_bytes;
            self.last_stats.image_argument_buffer_grows =
                self.acc_image_argument_buffer_grows;
            self.last_stats.render_passes = self.acc_render_passes;
            self.last_stats.blit_passes = self.acc_blit_passes;
            self.last_stats.command_buffers = self.frames[slot].cmd.is_some() as u32;
            self.last_stats.texture_copies = self.acc_texture_copies;
            self.last_stats.texture_copy_pixels = self.acc_texture_copy_pixels;
            self.last_stats.texture_copy_bytes = self.acc_texture_copy_bytes;
            self.last_stats.buffer_upload_bytes = (self.frames[slot].vb_used as u64)
                .saturating_add(self.frames[slot].ib_used as u64)
                .saturating_add(self.frames[slot].ub_used as u64);
            self.last_stats.geometry_bytes_copied = self
                .last_stats
                .geometry_bytes_copied
                .saturating_add(self.last_stats.buffer_upload_bytes);
            self.last_stats.shaded_damage_px =
                self.main_shaded_px.saturating_add(self.prepass_shaded_px);
            self.last_stats.resource_creates = self.acc_resource_creates;
            self.last_stats.resource_grows = self.acc_resource_grows;
            if self.memory_stats_enabled && self.frame_id.saturating_sub(1) % 60 == 0 {
                self.last_stats.memory = self.memory_stats();
            }
            self.last_stats.cache_bytes = self
                .last_stats
                .memory
                .layer_cache_bytes
                .saturating_add(self.last_stats.memory.image_cache_bytes)
                .saturating_add(self.id_mask_cache_resident_bytes)
                .saturating_add(self.last_stats.memory.id_mask_vertex_buffer_bytes)
                .saturating_add(self.last_stats.memory.prepared_cache_bytes);
        }
        self.last_stats.encode_ms = cpu_t0.elapsed().as_secs_f64() * 1000.0;
        self.last_stats.damage_px = self.frame_damage_px;
        self.last_stats.damage_pct = self.frame_damage_pct;
        self.last_stats.damage_rects = self.frame_damage_rects;
        self.last_stats.damage_forced_full_refreshes = self.acc_damage_forced_full_refreshes;
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
        let completed_gpu_stats = self.latest_completed_gpu_stats();
        self.last_stats.gpu_frame_id = completed_gpu_stats.frame_id;
        self.last_stats.gpu_ms = completed_gpu_stats.command_ms;
        self.last_stats.gpu_render_ms = completed_gpu_stats.render_ms;
        self.last_stats.gpu_vertex_ms = completed_gpu_stats.vertex_ms;
        self.last_stats.gpu_fragment_ms = completed_gpu_stats.fragment_ms;
        self.frame_2d_encoded = true;
        self.frame_color_initialized = true;
        if self.frame_present_direct_to_drawable {
            self.persistent_target_valid = false;
        } else {
            self.persistent_target_valid = true;
            self.persistent_target_policy = target_policy;
        }
        self.last_stats.persistent_target_valid = self.persistent_target_valid as u32;
        if let Some(t0) = self.frame_encode_started_at {
            self.last_stats.encode_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
        if renderer_trace_enabled() {
            renderer_trace_log(&format!(
                "OXIDE_METAL_TRACE phase=encode frame={} total_ms={:.3} draws={} instanced={} icb_cmds={} items={} vb_bytes={} ib_bytes={} ub_bytes={} damage_enabled={} use_damage={} damage_rects={} damage_pct={:.3} culled={} used_filtered_main={} direct_present={} auxiliary_textures={} persistent_final_target={} backdrop={} visual_effect={} camera_blur={} layer_commands={} scissor_changes={} main_shaded_px={} prepass_shaded_px={} gpu_ms={:.3} gpu_render_ms={:.3}",
                self.frame_id,
                self.last_stats.encode_ms,
                self.last_stats.draws,
                self.last_stats.instanced,
                self.last_stats.icb_cmds,
                list.items.len(),
                self.last_stats.vb_bytes,
                self.last_stats.ib_bytes,
                self.last_stats.ub_bytes,
                self.damage_enabled,
                use_damage,
                self.last_stats.damage_rects,
                self.last_stats.damage_pct,
                self.last_stats.culled,
                used_filtered_main,
                self.frame_present_direct_to_drawable,
                final_target_plan.needs_auxiliary_textures,
                final_target_plan.needs_persistent_final_target,
                has_backdrop,
                has_visual_effect,
                need_cam_blur,
                has_layer_commands,
                self.scissor_changes,
                self.main_shaded_px,
                self.prepass_shaded_px,
                self.last_stats.gpu_ms,
                self.last_stats.gpu_render_ms
            ));
        }
    }

    fn submit(&mut self, _token: api::FrameToken) -> Result<(), api::RenderError> {
        let trace = renderer_trace_enabled();
        let trace_started_at = if trace { Some(Instant::now()) } else { None };
        if self.submit_error_flag.swap(false, Ordering::AcqRel) {
            self.purge_layer_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
            self.purge_id_mask_field_cache();
            let detail = self.submit_error_detail.lock().ok().and_then(|mut slot| slot.take());
            if let Some(detail) = detail {
                return Err(api::RenderError::Io(format!("device lost: {}", detail)));
            }
            return Err(api::RenderError::DeviceLost);
        }
        let slot = self.current_frame_slot();
        let pending_present_drawable = self.pending_present_drawable as *mut core::ffi::c_void;
        let present_direct_to_drawable = self.frame_present_direct_to_drawable;
        let has_present_drawable = !pending_present_drawable.is_null();
        let blit_present_to_drawable =
            has_present_drawable && !present_direct_to_drawable && self.target_tex.is_some();
        self.pending_present_drawable = 0;
        self.pending_present_texture = 0;
        self.frame_present_direct_to_drawable = false;
        if self.accounting_stats_enabled {
            self.last_stats.commands_traversed = self.acc_commands_traversed;
            self.last_stats.commands_copied = self.acc_commands_copied;
            self.last_stats.geometry_bytes_copied = self
                .last_stats
                .geometry_bytes_copied
                .max(self.acc_geometry_bytes_copied);
            self.last_stats.chunks_reused = self.acc_chunks_reused;
            self.last_stats.chunks_rebuilt = self.acc_chunks_rebuilt;
            self.last_stats.chunks_prepared = self.acc_chunks_prepared;
            self.last_stats.backend_cache_hits = self.acc_backend_cache_hits;
            self.last_stats.backend_cache_misses = self.acc_backend_cache_misses;
            self.last_stats.layer_body_commands_scanned = self.acc_layer_body_commands_scanned;
            self.last_stats.layer_body_commands_copied = self.acc_layer_body_commands_copied;
            self.last_stats.layer_texture_creates = self.acc_layer_texture_creates;
            self.last_stats.layer_cache_hits = self.acc_layer_cache_hits;
            self.last_stats.layer_cache_misses = self.acc_layer_cache_misses;
            self.last_stats.layer_offscreen_draws = self.acc_layer_offscreen_draws;
            self.last_stats.layer_inline_draws = self.acc_layer_inline_draws;
            self.last_stats.layer_double_render_prevented =
                self.acc_layer_double_render_prevented;
            self.last_stats.image_argument_encodes = self.acc_image_argument_encodes;
            self.last_stats.image_argument_binds = self.acc_image_argument_binds;
            self.last_stats.image_argument_tables_finalized =
                self.acc_image_argument_tables_finalized;
            self.last_stats.image_argument_table_reuses =
                self.acc_image_argument_table_reuses;
            self.last_stats.image_argument_bytes = self.acc_image_argument_bytes;
            self.last_stats.image_argument_buffer_grows =
                self.acc_image_argument_buffer_grows;
            self.last_stats.render_passes = self.acc_render_passes;
            self.last_stats.resource_creates = self.acc_resource_creates;
            self.last_stats.resource_grows = self.acc_resource_grows;
            if self.memory_stats_enabled && self.frame_id.saturating_sub(1) % 60 == 0 {
                self.last_stats.memory = self.memory_stats();
            }
            self.last_stats.cache_bytes = self
                .last_stats
                .memory
                .layer_cache_bytes
                .saturating_add(self.last_stats.memory.image_cache_bytes)
                .saturating_add(self.id_mask_cache_resident_bytes)
                .saturating_add(self.last_stats.memory.id_mask_vertex_buffer_bytes)
                .saturating_add(self.last_stats.memory.prepared_cache_bytes);
        }
        if let Some(cmd) = self.frames[slot].cmd.take() {
            let frame_id = self.frame_id;
            let log_enabled = ios_log_enabled();
            let submit_error_flag = self.submit_error_flag.clone();
            let submit_error_detail = self.submit_error_detail.clone();
            let completed_gpu_stats = self.completed_gpu_stats.clone();
            let gpu_trace = self.frame_gpu_trace.take();
            let gpu_device = self.device.to_owned();
            let frame_in_flight = self.frame_in_flight.clone();
            let submitted_slot_bit = frame_slot_bit(slot);
            frame_in_flight.fetch_or(submitted_slot_bit, Ordering::Release);
            if !pending_present_drawable.is_null() {
                let raw_drawable = pending_present_drawable as *mut MTLDrawable;
                let drawable = unsafe { DrawableRef::from_ptr(raw_drawable) };
                if present_direct_to_drawable {
                    cmd.present_drawable(drawable);
                } else if let Some(src) = &self.target_tex {
                    let raw_drawable_obj = pending_present_drawable as *mut Object;
                    let raw_dst_tex: *mut MTLTexture =
                        unsafe { msg_send![raw_drawable_obj, texture] };
                    if raw_dst_tex.is_null() {
                        return Err(api::RenderError::InvalidOperation(
                            "drawable did not provide a destination texture",
                        ));
                    }
                    let dst = unsafe { TextureRef::from_ptr(raw_dst_tex) };
                    let blit = cmd.new_blit_command_encoder();
                    self.last_stats.blit_passes = self.last_stats.blit_passes.saturating_add(1);
                    let origin = MTLOrigin { x: 0, y: 0, z: 0 };
                    let copy_w = src.width().min(dst.width());
                    let copy_h = src.height().min(dst.height());
                    if copy_w == 0 || copy_h == 0 {
                        return Err(api::RenderError::InvalidOperation(
                            "zero-sized blit copy extent",
                        ));
                    }
                    let size = MTLSize { width: copy_w, height: copy_h, depth: 1 };
                    blit.copy_from_texture(src, 0, 0, origin, size, dst, 0, 0, origin);
                    let copy_pixels = copy_w.saturating_mul(copy_h);
                    self.last_stats.texture_copies =
                        self.last_stats.texture_copies.saturating_add(1);
                    self.last_stats.texture_copy_pixels = self
                        .last_stats
                        .texture_copy_pixels
                        .saturating_add(copy_pixels);
                    self.last_stats.texture_copy_bytes = self
                        .last_stats
                        .texture_copy_bytes
                        .saturating_add(copy_pixels.saturating_mul(4));
                    blit.end_encoding();
                    cmd.present_drawable(drawable);
                } else {
                    cmd.present_drawable(drawable);
                }
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
                    let detail = unsafe { command_buffer_error_detail(buffer) };
                    if let Ok(mut slot) = submit_error_detail.lock() {
                        *slot = detail.clone();
                    }
                    if log_enabled {
                        if let Some(detail) = detail {
                            ios_log(&format!(
                                "oxide.renderer-metal: submit error frame={} {}",
                                frame_id, detail
                            ));
                        } else {
                            ios_log(&format!(
                                "oxide.renderer-metal: submit error frame={} error=nil",
                                frame_id
                            ));
                        }
                    }
                    submit_error_flag.store(true, Ordering::Release);
                } else if status == MTLCommandBufferStatus::Completed {
                    let command_ms = unsafe { command_buffer_gpu_duration_ms(buffer) };
                    let stage_stats = gpu_trace
                        .as_ref()
                        .map(|trace| trace.resolve(&gpu_device))
                        .unwrap_or_default();
                    if let Ok(mut stats) = completed_gpu_stats.lock() {
                        *stats = CompletedGpuStats {
                            frame_id,
                            command_ms,
                            render_ms: stage_stats.render_ms,
                            vertex_ms: stage_stats.vertex_ms,
                            fragment_ms: stage_stats.fragment_ms,
                        };
                    }
                }
                frame_in_flight.fetch_and(!submitted_slot_bit, Ordering::Release);
            })
            .copy();
            cmd.add_completed_handler(&completion);
            cmd.commit();
            if self.accounting_stats_enabled {
                self.last_stats.actual_submissions =
                    self.last_stats.actual_submissions.saturating_add(1);
            }
            if trace {
                let total_ms = trace_started_at
                    .map(|start| start.elapsed().as_secs_f64() * 1000.0)
                    .unwrap_or(0.0);
                renderer_trace_log(&format!(
                    "OXIDE_METAL_TRACE phase=submit frame={} total_ms={:.3} had_command=1 slot={} present_drawable={} direct_present={} blit_present={} gpu_timestamps={}",
                    frame_id,
                    total_ms,
                    slot,
                    has_present_drawable,
                    present_direct_to_drawable,
                    blit_present_to_drawable,
                    self.gpu_stage_timing.is_some()
                ));
            }
        } else {
            if self.accounting_stats_enabled {
                self.last_stats.skipped_submissions =
                    self.last_stats.skipped_submissions.saturating_add(1);
            }
            if trace {
                let total_ms = trace_started_at
                    .map(|start| start.elapsed().as_secs_f64() * 1000.0)
                    .unwrap_or(0.0);
                renderer_trace_log(&format!(
                    "OXIDE_METAL_TRACE phase=submit frame={} total_ms={:.3} had_command=0 slot={} present_drawable={} direct_present={} blit_present={} gpu_timestamps={}",
                    self.frame_id,
                    total_ms,
                    slot,
                    has_present_drawable,
                    present_direct_to_drawable,
                    blit_present_to_drawable,
                    self.gpu_stage_timing.is_some()
                ));
            }
        }
        self.age_layer_cache();
        self.apply_layer_cache_stats();
        self.apply_id_mask_cache_stats();
        Ok(())
    }

    fn resize(&mut self, w: u32, h: u32, scale: f32) -> Result<(), api::RenderError> {
        let next_w = w.max(1);
        let next_h = h.max(1);
        let next_scale = if scale > 0.0 { scale } else { 1.0 };
        if self.target_w == next_w
            && self.target_h == next_h
            && (self.target_scale - next_scale).abs() <= f32::EPSILON
        {
            return Ok(());
        }
        let target_size_changed = self.target_w != next_w || self.target_h != next_h;
        let target_scale_changed = (self.target_scale - next_scale).abs() > f32::EPSILON;
        self.target_w = next_w;
        self.target_h = next_h;
        self.target_scale = next_scale;
        self.persistent_target_valid = false;
        if target_size_changed {
            self.target_tex = None;
            self.target_msaa_tex = None;
            self.depth_tex = None;
            self.purge_effect_targets();
        }
        if target_scale_changed {
            self.purge_layer_cache_for_reason(LAYER_PURGE_SCALE_CHANGE);
            self.purge_prepared_chunks();
        }
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

pub(crate) fn set_viewport_and_scissor_dp(
    enc: &RenderCommandEncoderRef,
    r: &MetalRenderer,
    rect: api::RectF,
) {
    let scale = r.target_scale.max(1.0);
    let x = (rect.x * scale).floor().max(0.0);
    let y = (rect.y * scale).floor().max(0.0);
    let w = (rect.w * scale).ceil().max(0.0);
    let h = (rect.h * scale).ceil().max(0.0);
    let target_w = r.target_w as f64;
    let target_h = r.target_h as f64;
    let x1 = (x as f64).clamp(0.0, target_w);
    let y1 = (y as f64).clamp(0.0, target_h);
    let x2 = ((x + w) as f64).clamp(0.0, target_w);
    let y2 = ((y + h) as f64).clamp(0.0, target_h);
    let width = (x2 - x1).max(0.0);
    let height = (y2 - y1).max(0.0);
    enc.set_viewport(MTLViewport {
        originX: x1,
        originY: y1,
        width,
        height,
        znear: 0.0,
        zfar: 1.0,
    });
    enc.set_scissor_rect(MTLScissorRect {
        x: x1 as u64,
        y: y1 as u64,
        width: width as u64,
        height: height as u64,
    });
}

fn encode_draws(
    enc: &RenderCommandEncoderRef,
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    list: DrawListView<'_>,
    prepass: bool,
    global_scissor_dp: Option<api::RectI>,
) {
    encode_draws_range(enc, pf, r, list, 0, list.items.len(), prepass, global_scissor_dp);
}

fn encode_cached_layer(
    enc: &RenderCommandEncoderRef,
    r: &mut MetalRenderer,
    plan: LayerPlan,
    rect: api::RectF,
    viewport: [f32; 2],
) {
    r.layer_frame_ids.insert(plan.id);
    let color_format = r.color_format;
    let frame_id = r.frame_id;
    let Some(layer) = r
        .layers
        .get_mut(&plan.id)
        .filter(|entry| {
            entry.generation == plan.generation
                && entry.tex.pixel_format() == color_format
                && entry.prepared_key.is_none()
        })
    else {
        debug_assert!(false, "planned Metal layer generation must exist before composition");
        return;
    };
    layer.last_used_frame = frame_id;
    let scale = r.target_scale.max(1.0);
    let pixel_aligned = !plan.refresh
        && [rect.x, rect.y, rect.w, rect.h]
        .into_iter()
        .all(|value| {
            let pixels = value * scale;
            (pixels - pixels.round()).abs() <= f32::EPSILON
        })
        && (rect.w * scale).round() as u32 == layer.w
        && (rect.h * scale).round() as u32 == layer.h;
    enc.set_render_pipeline_state(if pixel_aligned {
        &r.pso_layer_composite_aligned
    } else {
        &r.pso_layer_composite
    });
    if !pixel_aligned {
        if let Some(sampler) = &r.sampler {
            enc.set_fragment_sampler_state(0, Some(sampler));
        }
    }
    enc.set_fragment_texture(0, Some(&layer.tex));
    enc.set_vertex_bytes(
        1,
        core::mem::size_of_val(&viewport) as u64,
        viewport.as_ptr().cast(),
    );
    let vertex = [rect.x, rect.y, rect.w, rect.h, viewport[0], viewport[1]];
    enc.set_vertex_bytes(0, core::mem::size_of_val(&vertex) as u64, vertex.as_ptr().cast());
    let fragment = pack_nine_slice_params(
        rect,
        layer.w as f32,
        layer.h as f32,
        api::Insets::new(0.0, 0.0, 0.0, 0.0),
        1.0,
    );
    enc.set_fragment_bytes(
        1,
        core::mem::size_of_val(&fragment) as u64,
        (&fragment as *const NineSliceGpuParams).cast(),
    );
    enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
    r.acc_draws = r.acc_draws.saturating_add(1);
}

fn encode_inline_layer(
    enc: &RenderCommandEncoderRef,
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    list: DrawListView<'_>,
    item_start: usize,
    item_end: usize,
    global_scissor_dp: Option<api::RectI>,
) {
    let owns_counter = !r.inline_layer_counter_active;
    let draws_before = u64::from(r.acc_draws).saturating_add(u64::from(r.acc_instanced));
    r.inline_layer_counter_active = true;
    encode_draws_range(enc, pf, r, list, item_start, item_end, false, global_scissor_dp);
    r.inline_layer_counter_active = !owns_counter;
    if owns_counter {
        let draws_after = u64::from(r.acc_draws).saturating_add(u64::from(r.acc_instanced));
        r.acc_layer_inline_draws = r
            .acc_layer_inline_draws
            .saturating_add(draws_after.saturating_sub(draws_before));
    }
}

#[inline]
fn analytic_instance_pair_layout<V, F>(start: usize, count: usize) -> (usize, usize, usize)
{
    let vertex_offset = align_up_usize(start, 16.max(core::mem::align_of::<V>()));
    let vertex_bytes = count.saturating_mul(core::mem::size_of::<V>());
    let fragment_offset = align_up_usize(
        vertex_offset.saturating_add(vertex_bytes),
        16.max(core::mem::align_of::<F>()),
    );
    let fragment_bytes = count.saturating_mul(core::mem::size_of::<F>());
    let end = fragment_offset.saturating_add(fragment_bytes);
    (vertex_offset, fragment_offset, end)
}

#[inline]
fn reserve_analytic_instance_pair<V, F>(
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    count: usize,
) -> (usize, usize) {
    let (vertex_offset, fragment_offset, end) =
        analytic_instance_pair_layout::<V, F>(pf.ub_used, count);
    let slot = r.current_frame_slot();
    if r.ub.ensure_capacity(&r.device, slot, end) {
        r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
        r.acc_analytic_instance_ring_grows =
            r.acc_analytic_instance_ring_grows.saturating_add(1);
    }
    pf.ub_used = end;
    r.acc_analytic_instance_bytes = r
        .acc_analytic_instance_bytes
        .saturating_add(
            count
                .saturating_mul(core::mem::size_of::<V>() + core::mem::size_of::<F>())
                as u64,
        );
    (vertex_offset, fragment_offset)
}

#[inline]
fn reserve_analytic_instance_slice<T>(
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    count: usize,
) -> usize {
    let offset = align_up_usize(pf.ub_used, 16.max(core::mem::align_of::<T>()));
    let bytes = count.saturating_mul(core::mem::size_of::<T>());
    let end = offset.saturating_add(bytes);
    let slot = r.current_frame_slot();
    if r.ub.ensure_capacity(&r.device, slot, end) {
        r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
        r.acc_analytic_instance_ring_grows =
            r.acc_analytic_instance_ring_grows.saturating_add(1);
    }
    pf.ub_used = end;
    r.acc_analytic_instance_bytes =
        r.acc_analytic_instance_bytes.saturating_add(bytes as u64);
    offset
}

#[inline]
unsafe fn write_ring_value<T>(base: NonNull<u8>, offset: usize, index: usize, value: T)
{
    unsafe {
        base.as_ptr()
            .add(offset)
            .cast::<T>()
            .add(index)
            .write(value);
    }
}

fn encode_draws_range(
    enc: &RenderCommandEncoderRef,
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    list: DrawListView<'_>,
    item_start: usize,
    item_end: usize,
    prepass: bool,
    global_scissor_dp: Option<api::RectI>,
) {
    let debug_stride = encode_debug_stride();
    let slot = r.current_frame_slot();
    // Scissor state
    let mut stack = r.clip_stack_pool.pop().unwrap_or_default();
    stack.clear();
    let mut current: Option<api::RectI> = None;
    let mut last_applied: Option<api::RectI> = None;

    let vp_dp: [f32; 2] = [
        (r.target_w as f32) / r.target_scale.max(1.0),
        (r.target_h as f32) / r.target_scale.max(1.0),
    ];

    let mut i: usize = item_start;
    while i < item_end {
        if debug_stride > 0 && (i == 0 || (i % debug_stride) == 0) {
            ios_log(&format!(
                "oxide.renderer-metal: encode frame={} prepass={} idx={} total={} kind={}",
                r.frame_id,
                prepass,
                i,
                item_end.saturating_sub(item_start),
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
                    let fparams = pack_rrect_params(
                        *rect,
                        [0.0, 0.0, 0.0, 0.0],
                        api::Color::rgba(tint.r, tint.g, tint.b, a),
                    );
                    let pipeline =
                        if r.encoding_layer { &r.pso_layer_rrect } else { &r.pso_rrect };
                    enc.set_render_pipeline_state(pipeline);
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
                        (&fparams as *const RRectGpuParams).cast(),
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
            api::DrawCmd::LayerBegin { id, rect, dirty: _ } => {
                let plan = if prepass { None } else { r.layer_plan(*id, i) };
                let planned_end = plan.and_then(|plan| {
                    let end = i.checked_add(plan.end.saturating_sub(plan.begin))?;
                    matches!(list.items.get(end), Some(api::DrawCmd::LayerEnd)).then_some(end)
                });
                let end = if let Some(end) = planned_end {
                    end
                } else {
                    let mut depth = 1usize;
                    let mut cursor = i + 1;
                    while cursor < item_end && depth > 0 {
                        match &list.items[cursor] {
                            api::DrawCmd::LayerBegin { .. } => depth += 1,
                            api::DrawCmd::LayerEnd => depth -= 1,
                            _ => {}
                        }
                        cursor += 1;
                    }
                    cursor.saturating_sub(1)
                };
                if prepass {
                    let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                    encode_draws_range(enc, pf, r, list, i + 1, end, true, global_scissor_dp);
                    apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
                    i = end + 1;
                    continue;
                }
                if let Some(plan) = plan {
                    match plan.action {
                        LayerPlanAction::Inline => {
                            let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                            encode_inline_layer(
                                enc,
                                pf,
                                r,
                                list,
                                i + 1,
                                end,
                                global_scissor_dp,
                            );
                            apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
                        }
                        LayerPlanAction::Composite => {
                            encode_cached_layer(enc, r, plan, *rect, vp_dp);
                        }
                    }
                    i = end + 1;
                    continue;
                }
                let resume_scissor = effective_scissor_dp(current, global_scissor_dp);
                encode_inline_layer(enc, pf, r, list, i + 1, end, global_scissor_dp);
                apply_scissor_dp(enc, r, resume_scissor, &mut last_applied);
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
                stack.push(next);
                current = Some(next);
                let effective = effective_scissor_dp(current, global_scissor_dp);
                apply_scissor_dp(enc, r, effective, &mut last_applied);
                i += 1;
                continue;
            }
            api::DrawCmd::ClipPop => {
                let _ = stack.pop();
                current = stack.last().copied();
                let effective = effective_scissor_dp(current, global_scissor_dp);
                apply_scissor_dp(enc, r, effective, &mut last_applied);
                i += 1;
                continue;
            }
            api::DrawCmd::Solid { vb, ib, color } => {
                enc.set_render_pipeline_state(&r.pso_solid);
                let v_count = vb.len as usize;
                let v_bytes = v_count * core::mem::size_of::<api::Vertex>();
                let slot = r.current_frame_slot();
                if r.vb.ensure_capacity(&r.device, slot, pf.vb_used + v_bytes) {
                    r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                }
                let src_slice =
                    &list.vertices[(vb.offset as usize)..(vb.offset as usize + v_count)];
                let dst_vertices = unsafe {
                    core::slice::from_raw_parts_mut(
                        r.vb.contents_ptr(slot).as_ptr().add(pf.vb_used) as *mut api::Vertex,
                        v_count,
                    )
                };
                for (dst, vertex) in dst_vertices.iter_mut().zip(src_slice.iter().copied()) {
                    *dst = map_solid_vertex_dp_to_clip(vertex, vp_dp[0], vp_dp[1]);
                }
                let vb_off = pf.vb_used as u64;
                pf.vb_used += v_bytes;
                let rgba = [color.r, color.g, color.b, color.a];
                let ub_off = pf.ub_used as u64;
                let u_bytes = core::mem::size_of_val(&rgba);
                if r.ub.ensure_capacity(&r.device, slot, pf.ub_used + u_bytes) {
                    r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        rgba.as_ptr() as *const u8,
                        r.ub.contents_ptr(slot).as_ptr().add(pf.ub_used),
                        u_bytes,
                    );
                }
                pf.ub_used += u_bytes;
                enc.set_vertex_buffer(0, Some(r.vb.buffer(slot)), vb_off);
                enc.set_vertex_buffer(1, Some(r.ub.buffer(slot)), ub_off);
                let idx_count = ib.len as usize;
                if idx_count > 0 {
                    // Upload indices and draw indexed
                    let isrc_slice =
                        &list.indices[(ib.offset as usize)..(ib.offset as usize + idx_count)];
                    let i_bytes = isrc_slice.len() * core::mem::size_of::<u16>();
                    if r.ib.ensure_capacity(&r.device, slot, pf.ib_used + i_bytes) {
                        r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                    }
                    let idst = unsafe {
                        core::slice::from_raw_parts_mut(
                            r.ib.contents_ptr(slot).as_ptr().add(pf.ib_used),
                            i_bytes,
                        )
                    };
                    let Some(local_idx_count) = copy_normalized_indices_for_local_vertex_span(
                        isrc_slice, vb.offset, vb.len, idst,
                    ) else {
                        i += 1;
                        continue;
                    };
                    let ib_off = pf.ib_used as u64;
                    pf.ib_used += i_bytes;
                    if let Some(primitive) = solid_primitive_for_index_count(local_idx_count) {
                        enc.draw_indexed_primitives(
                            primitive,
                            local_idx_count as u64,
                            MTLIndexType::UInt16,
                            r.ib.buffer(slot),
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
            api::DrawCmd::RRect { .. } => {
                let pipeline = if r.encoding_layer { &r.pso_layer_rrect } else { &r.pso_rrect };
                enc.set_render_pipeline_state(pipeline);
                let mut j = i;
                while j < item_end && matches!(list.items[j], api::DrawCmd::RRect { .. }) {
                    j += 1;
                }
                let count = j - i;
                let instance_offset =
                    reserve_analytic_instance_slice::<RRectGpuParams>(pf, r, count);
                let base = r.ub.contents_ptr(slot);
                for (index, item) in list.items[i..j].iter().enumerate() {
                    let api::DrawCmd::RRect { rect, radii, color } = item else { unreachable!() };
                    unsafe {
                        write_ring_value(
                            base,
                            instance_offset,
                            index,
                            pack_rrect_params(*rect, *radii, *color),
                        );
                    }
                }
                enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), instance_offset as u64);
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), instance_offset as u64);
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                r.acc_analytic_instance_buffer_binds =
                    r.acc_analytic_instance_buffer_binds.saturating_add(2);
                r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                i = j;
                continue;
            }
            api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                if let Some(img) = r.get_image_tex(*tex) {
                    let pipeline = if r.encoding_layer {
                        &r.pso_layer_nine_slice
                    } else {
                        &r.pso_nine_slice
                    };
                    enc.set_render_pipeline_state(pipeline);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(img));
                    let tex_w = img.width() as f32;
                    let tex_h = img.height() as f32;
                    let mut j = i;
                    while j < item_end {
                        if let api::DrawCmd::NineSlice { tex: t2, rect, slice, alpha } =
                            &list.items[j]
                        {
                            if *t2 != *tex {
                                break;
                            }
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let count = j - i;
                    let instance_offset =
                        reserve_analytic_instance_slice::<NineSliceGpuParams>(pf, r, count);
                    let base = r.ub.contents_ptr(slot);
                    for (index, item) in list.items[i..j].iter().enumerate() {
                        let api::DrawCmd::NineSlice { rect, slice, alpha, .. } = item
                        else
                        {
                            unreachable!()
                        };
                        unsafe {
                            write_ring_value(
                                base,
                                instance_offset,
                                index,
                                pack_nine_slice_params(*rect, tex_w, tex_h, *slice, *alpha),
                            );
                        }
                    }
                    enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), instance_offset as u64);
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), instance_offset as u64);
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                    r.acc_analytic_instance_buffer_binds =
                        r.acc_analytic_instance_buffer_binds.saturating_add(2);
                    r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                    r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                    i = j;
                    continue;
                }
                i += 1;
            }
            api::DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
                if let Some(img) = r.get_image_tex(*tex) {
                    let v_count = vb.len as usize;
                    let Some(src_slice) =
                        list.vertices.get(vb.offset as usize..vb.offset as usize + v_count)
                    else {
                        i += 1;
                        continue;
                    };
                    let pipeline =
                        if r.encoding_layer { &r.pso_layer_image_mesh } else { &r.pso_image_mesh };
                    enc.set_render_pipeline_state(pipeline);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(img));
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );

                    let v_bytes = v_count * core::mem::size_of::<api::Vertex>();
                    if r.vb.ensure_capacity(&r.device, slot, pf.vb_used + v_bytes) {
                        r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                    }
                    let dst = unsafe {
                        core::slice::from_raw_parts_mut(
                            r.vb.contents_ptr(slot).as_ptr().add(pf.vb_used),
                            v_bytes,
                        )
                    };
                    let src_bytes =
                        unsafe { core::slice::from_raw_parts(src_slice.as_ptr().cast(), v_bytes) };
                    dst.copy_from_slice(src_bytes);
                    let vb_off = pf.vb_used as u64;
                    pf.vb_used += v_bytes;

                    let rgba = [1.0_f32, 1.0, 1.0, alpha.clamp(0.0, 1.0)];
                    let ub_off = pf.ub_used as u64;
                    let u_bytes = core::mem::size_of_val(&rgba);
                    if r.ub.ensure_capacity(&r.device, slot, pf.ub_used + u_bytes) {
                        r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                    }
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            rgba.as_ptr().cast::<u8>(),
                            r.ub.contents_ptr(slot).as_ptr().add(pf.ub_used),
                            u_bytes,
                        );
                    }
                    pf.ub_used += u_bytes;
                    enc.set_vertex_buffer(0, Some(r.vb.buffer(slot)), vb_off);
                    enc.set_fragment_buffer(0, Some(r.ub.buffer(slot)), ub_off);

                    let idx_count = ib.len as usize;
                    if idx_count > 0 {
                        let Some(isrc_slice) =
                            list.indices.get(ib.offset as usize..ib.offset as usize + idx_count)
                        else {
                            i += 1;
                            continue;
                        };
                        let i_bytes = isrc_slice.len() * core::mem::size_of::<u16>();
                        if r.ib.ensure_capacity(&r.device, slot, pf.ib_used + i_bytes) {
                            r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                        }
                        let idst = unsafe {
                            core::slice::from_raw_parts_mut(
                                r.ib.contents_ptr(slot).as_ptr().add(pf.ib_used),
                                i_bytes,
                            )
                        };
                        let Some(local_idx_count) = copy_normalized_indices_for_local_vertex_span(
                            isrc_slice, vb.offset, vb.len, idst,
                        ) else {
                            i += 1;
                            continue;
                        };
                        let ib_off = pf.ib_used as u64;
                        pf.ib_used += i_bytes;
                        enc.draw_indexed_primitives(
                            MTLPrimitiveType::Triangle,
                            local_idx_count as u64,
                            MTLIndexType::UInt16,
                            r.ib.buffer(slot),
                            ib_off,
                        );
                        r.acc_draws = r.acc_draws.saturating_add(1);
                    } else if let Some(primitive) = solid_primitive_for_vertex_count(v_count) {
                        enc.draw_primitives(primitive, 0, v_count as u64);
                        r.acc_draws = r.acc_draws.saturating_add(1);
                    }
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
                    let pipeline = if r.encoding_layer {
                        &r.pso_layer_image_single
                    } else {
                        &r.pso_image_single
                    };
                    enc.set_render_pipeline_state(pipeline);
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    let api::DrawCmd::Image { tex: first_tex, .. } = &list.items[i]
                    else
                    {
                        unreachable!()
                    };
                    let Some(texture) = r.get_image_tex(*first_tex).map(Texture::to_owned)
                    else
                    {
                        i += 1;
                        continue;
                    };
                    let mut j = i;
                    while j < item_end {
                        if let api::DrawCmd::Image { tex, .. } = &list.items[j] {
                            if *tex != *first_tex {
                                break;
                            }
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let count = j - i;
                    let (vertex_offset, fragment_offset) =
                        reserve_analytic_instance_pair::<[f32; 4], ImageGpuParams>(pf, r, count);
                    let base = r.ub.contents_ptr(slot);
                    let texture_size = [texture.width() as f32, texture.height() as f32];
                    for (index, item) in list.items[i..j].iter().enumerate() {
                        let api::DrawCmd::Image { dst, src, alpha, .. } = item
                        else
                        {
                            unreachable!()
                        };
                        unsafe {
                            write_ring_value(
                                base,
                                vertex_offset,
                                index,
                                [dst.x, dst.y, dst.w, dst.h],
                            );
                            write_ring_value(
                                base,
                                fragment_offset,
                                index,
                                pack_image_params(*dst, *src, texture_size, *alpha, 0),
                            );
                        }
                    }
                    enc.set_fragment_texture(0, Some(&texture));
                    enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), vertex_offset as u64);
                    enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), fragment_offset as u64);
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                    r.acc_analytic_instance_buffer_binds =
                        r.acc_analytic_instance_buffer_binds.saturating_add(2);
                    r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                    r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                    i = j;
                    continue;
                }

                let pipeline =
                    if r.encoding_layer { &r.pso_layer_image } else { &r.pso_image };
                enc.set_render_pipeline_state(pipeline);
                // Batch consecutive Images regardless of texture using argument buffer
                let mut count = 0usize;
                r.image_tex_map.clear();
                r.image_arg_handles.clear();
                r.image_vbuf.clear();
                r.image_fbuf.clear();
                let mut next_slot: u32 = 0;
                let mut j = i;
                while j < item_end {
                    if let api::DrawCmd::Image { tex, dst, src, alpha } = &list.items[j] {
                        let existing_slot = r.image_tex_map.get(&tex.0).copied();
                        let Some(tref) = r.get_image_tex(*tex) else {
                            // Skip image draws referencing unknown textures to avoid sampling
                            // unbound argument-buffer slots on simulator/device GPUs.
                            j += 1;
                            continue;
                        };
                        let texture_size = [tref.width() as f32, tref.height() as f32];
                        // Map texture handle to slot
                        let slot_idx = if let Some(slot) = existing_slot {
                            slot
                        } else {
                            if next_slot >= IMAGE_ARG_TEXTURE_SLOTS {
                                break;
                            }
                            let s = next_slot;
                            next_slot += 1;
                            enc.use_resource_at(
                                tref,
                                MTLResourceUsage::Read,
                                MTLRenderStages::Fragment,
                            );
                            r.image_tex_map.insert(tex.0, s);
                            r.image_arg_handles.push(tex.0);
                            s
                        };
                        r.image_vbuf.extend_from_slice(&[dst.x, dst.y, dst.w, dst.h]);
                        r.image_fbuf.push(pack_image_params(
                            *dst,
                            *src,
                            texture_size,
                            *alpha,
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
                let table_key = if r.image_arg_table_count < IMAGE_ARG_SMALL_TABLE_COUNT
                {
                    None
                }
                else
                {
                    if r.image_arg_table_index.is_empty()
                    {
                        for index in 0..r.image_arg_table_count
                        {
                            let key = image_argument_table_key(&r.image_arg_tables[index].handles);
                            r.image_arg_table_index.insert(key, index);
                        }
                    }
                    Some(image_argument_table_key(&r.image_arg_handles))
                };
                let reused_table = if let Some(table_key) = table_key
                {
                    r.image_arg_table_index
                        .get(&table_key)
                        .copied()
                        .filter(|index| r.image_arg_tables[*index].handles == r.image_arg_handles)
                }
                else
                {
                    (0..r.image_arg_table_count)
                        .find(|index| r.image_arg_tables[*index].handles == r.image_arg_handles)
                };
                let table_index = if let Some(index) = reused_table
                {
                    r.acc_image_argument_table_reuses =
                        r.acc_image_argument_table_reuses.saturating_add(1);
                    index
                }
                else
                {
                    let index = r.image_arg_table_count;
                    if index == r.image_arg_tables.len()
                    {
                        r.image_arg_tables.push(ImageArgTable::default());
                    }
                    let offset = r.img_arg_used;
                    let needed = r.img_arg_used.saturating_add(r.img_arg_stride);
                    r.ensure_image_argument_capacity(slot, needed);
                    r.img_arg_used = needed;
                    let (Some(argument_encoder), Some(argument_buffers)) =
                        (r.img_arg.as_ref(), r.img_arg_bufs.as_ref())
                    else
                    {
                        i = j;
                        continue;
                    };
                    debug_assert!(r.img_arg_used <= argument_buffers.cap[slot]);
                    let buffer = argument_buffers.buffer(slot);
                    argument_encoder.set_argument_buffer(buffer, offset as u64);
                    for (texture_index, handle) in r.image_arg_handles.iter().copied().enumerate()
                    {
                        if let Some(texture) = r.images.get(&handle)
                        {
                            argument_encoder.set_texture(texture_index as u64, texture);
                        }
                    }
                    let table = &mut r.image_arg_tables[index];
                    table.handles.clear();
                    table.handles.extend_from_slice(&r.image_arg_handles);
                    table.offset = offset as u64;
                    r.image_arg_table_count += 1;
                    if let Some(table_key) = table_key
                    {
                        r.image_arg_table_index.insert(table_key, index);
                    }
                    r.acc_image_argument_encodes =
                        r.acc_image_argument_encodes.saturating_add(1);
                    r.acc_image_argument_tables_finalized =
                        r.acc_image_argument_tables_finalized.saturating_add(1);
                    r.acc_image_argument_bytes = r
                        .acc_image_argument_bytes
                        .saturating_add(r.img_arg_stride as u64);
                    index
                };
                let Some(argument_buffers) = r.img_arg_bufs.as_ref()
                else
                {
                    i = j;
                    continue;
                };
                enc.set_fragment_buffer(
                    2,
                    Some(argument_buffers.buffer(slot)),
                    r.image_arg_tables[table_index].offset,
                );
                r.acc_image_argument_binds = r.acc_image_argument_binds.saturating_add(1);
                let (vertex_offset, fragment_offset) =
                    reserve_analytic_instance_pair::<[f32; 4], ImageGpuParams>(pf, r, count);
                let base = r.ub.contents_ptr(slot);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        r.image_vbuf.as_ptr().cast::<u8>(),
                        base.as_ptr().add(vertex_offset),
                        r.image_vbuf.len() * core::mem::size_of::<f32>(),
                    );
                    core::ptr::copy_nonoverlapping(
                        r.image_fbuf.as_ptr().cast::<u8>(),
                        base.as_ptr().add(fragment_offset),
                        r.image_fbuf.len() * core::mem::size_of::<ImageGpuParams>(),
                    );
                }
                enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), vertex_offset as u64);
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), fragment_offset as u64);
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                r.acc_analytic_instance_buffer_binds =
                    r.acc_analytic_instance_buffer_binds.saturating_add(2);
                r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                i = j;
                continue;
            }
            api::DrawCmd::GlyphRun { .. } => {
                let api::DrawCmd::GlyphRun { run: first } = list.items[i] else {
                    unreachable!();
                };
                let group_atlas = first.atlas;
                let group_sdf = first.sdf;
                r.glyph_instances.clear();
                let mut j = i;
                while let Some(api::DrawCmd::GlyphRun { run }) = list.items.get(j) {
                    if j >= item_end
                        || run.atlas != group_atlas
                        || run.sdf != group_sdf
                    {
                        break;
                    }
                    if append_glyph_instances(
                        list.vertices,
                        list.indices,
                        *run,
                        &mut r.glyph_instances,
                    )
                    .is_none()
                    {
                        break;
                    }
                    j += 1;
                }
                let count = r.glyph_instances.len();
                if ios_log_enabled() {
                    ios_log(&format!(
                        "oxide.renderer-metal: glyph group instances={} atlas_handle={} sdf={}",
                        count,
                        group_atlas.0,
                        group_sdf
                    ));
                }
                if count > 0 {
                    let byte_count = count * core::mem::size_of::<GlyphGpuInstance>();
                    let instance_offset = align_up_usize(pf.ub_used, 16);
                    if r.ub.ensure_capacity(&r.device, slot, instance_offset + byte_count) {
                        r.acc_resource_grows = r.acc_resource_grows.saturating_add(1);
                    }
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            r.glyph_instances.as_ptr().cast::<u8>(),
                            r.ub.contents_ptr(slot).as_ptr().add(instance_offset),
                            byte_count,
                        );
                    }
                    pf.ub_used = instance_offset + byte_count;
                    if let Some(atlas) = r.get_image_tex(group_atlas) {
                        if ios_log_enabled() {
                            ios_log(&format!(
                                "oxide.renderer-metal: glyph atlas bound={}x{}",
                                atlas.width(),
                                atlas.height()
                            ));
                        }
                        if group_sdf {
                            let pipeline = if r.encoding_layer {
                                &r.pso_layer_text_sdf
                            } else {
                                &r.pso_text_sdf
                            };
                            enc.set_render_pipeline_state(pipeline);
                        } else {
                            let pipeline =
                                if r.encoding_layer { &r.pso_layer_text } else { &r.pso_text };
                            enc.set_render_pipeline_state(pipeline);
                        }
                        if let Some(sam) = &r.sampler {
                            enc.set_fragment_sampler_state(0, Some(sam));
                        }
                        enc.set_fragment_texture(0, Some(atlas));
                        enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), instance_offset as u64);
                        enc.set_vertex_bytes(
                            1,
                            core::mem::size_of_val(&vp_dp) as u64,
                            vp_dp.as_ptr() as *const _,
                        );
                        enc.draw_primitives_instanced(
                            MTLPrimitiveType::TriangleStrip,
                            0,
                            4,
                            count as u64,
                        );
                        r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                        r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                        r.acc_glyph_instance_bytes = r
                            .acc_glyph_instance_bytes
                            .saturating_add(byte_count as u64);
                        r.acc_glyph_instance_buffer_binds =
                            r.acc_glyph_instance_buffer_binds.saturating_add(1);
                        r.acc_glyph_instances = r.acc_glyph_instances.saturating_add(count as u32);
                    } else if ios_log_enabled() {
                        ios_log(&format!(
                            "oxide.renderer-metal: glyph atlas missing for handle={}",
                            group_atlas.0
                        ));
                    }
                }
                if j == i {
                    j += 1;
                }
                i = j;
                continue;
            }
            api::DrawCmd::Spinner { center, atom, alpha } => {
                let pipeline =
                    if r.encoding_layer { &r.pso_layer_spinner } else { &r.pso_spinner };
                enc.set_render_pipeline_state(pipeline);
                let phase = legacy_spinner_phase(spinner_now_ms());
                let mut j = i;
                while j < item_end && matches!(&list.items[j], api::DrawCmd::Spinner { .. }) {
                    j += 1;
                }
                let count = j - i;
                let (vertex_offset, fragment_offset) =
                    reserve_analytic_instance_pair::<[f32; 4], SpinnerGpuParams>(pf, r, count);
                let base = r.ub.contents_ptr(slot);
                for (index, item) in list.items[i..j].iter().enumerate() {
                    if let api::DrawCmd::Spinner { center, atom, alpha } = item {
                        let thickness = legacy_spinner_thickness(*atom);
                        let radius = legacy_spinner_radius(*atom);
                        let mm = *atom * 0.5;
                        unsafe {
                            write_ring_value(
                                base,
                                vertex_offset,
                                index,
                                [center[0] - mm, center[1] - mm, mm * 2.0, mm * 2.0],
                            );
                            write_ring_value(
                                base,
                                fragment_offset,
                                index,
                                pack_spinner_params(*center, radius, thickness, phase, *alpha),
                            );
                        }
                    }
                }
                enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), vertex_offset as u64);
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), fragment_offset as u64);
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                r.acc_analytic_instance_buffer_binds =
                    r.acc_analytic_instance_buffer_binds.saturating_add(2);
                r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
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
                    r.effect_fbuf.clear();
                    let mut j = i;
                    while j < item_end {
                        let api::DrawCmd::Backdrop { rect, tint, alpha, .. } = &list.items[j]
                        else
                        {
                            break;
                        };
                        let a = (tint.a * *alpha).clamp(0.0, 1.0);
                        r.effect_fbuf.extend_from_slice(&[
                            rect.x, rect.y, rect.w, rect.h, tint.r, tint.g, tint.b, a,
                        ]);
                        j += 1;
                    }
                    let count = j - i;
                    let instance_offset =
                        reserve_analytic_instance_slice::<[f32; 8]>(pf, r, count);
                    let base = r.ub.contents_ptr(slot);
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            r.effect_fbuf.as_ptr().cast::<u8>(),
                            base.as_ptr().add(instance_offset),
                            r.effect_fbuf.len() * core::mem::size_of::<f32>(),
                        );
                    }
                    enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), instance_offset as u64);
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), instance_offset as u64);
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                    r.acc_analytic_instance_buffer_binds =
                        r.acc_analytic_instance_buffer_binds.saturating_add(2);
                    r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                    r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                    i = j;
                    continue;
                }
                i += 1;
            }
            api::DrawCmd::VisualEffect { .. } => {
                if prepass {
                    // Stop prepass at the first visual effect; draw nothing for it here.
                    break;
                }
                if let Some(src) = &r.prepass_tex {
                    enc.set_render_pipeline_state(&r.pso_visual_effect);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(src));
                    r.effect_fbuf.clear();
                    let mut j = i;
                    while j < item_end {
                        let api::DrawCmd::VisualEffect { rect, effect } = &list.items[j]
                        else
                        {
                            break;
                        };
                        r.effect_fbuf
                            .extend_from_slice(&pack_visual_effect_params(*rect, *effect));
                        j += 1;
                    }
                    let count = j - i;
                    let instance_offset =
                        reserve_analytic_instance_slice::<[f32; 8]>(pf, r, count);
                    let base = r.ub.contents_ptr(slot);
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            r.effect_fbuf.as_ptr().cast::<u8>(),
                            base.as_ptr().add(instance_offset),
                            r.effect_fbuf.len() * core::mem::size_of::<f32>(),
                        );
                    }
                    enc.set_vertex_buffer(0, Some(r.ub.buffer(slot)), instance_offset as u64);
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_fragment_buffer(1, Some(r.ub.buffer(slot)), instance_offset as u64);
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                    r.acc_analytic_instance_buffer_binds =
                        r.acc_analytic_instance_buffer_binds.saturating_add(2);
                    r.acc_flat_instanced_draws = r.acc_flat_instanced_draws.saturating_add(1);
                    r.acc_instanced = r.acc_instanced.saturating_add(count as u32);
                    i = j;
                    continue;
                }
                i += 1;
            } // ClipPush/ClipPop handled above
        }
        // Default progress
        // Note: continue branches have updated i accordingly
        if i < item_end { /* fallthrough increment happens in each arm */ }
    }
    r.clip_stack_pool.push(stack);
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
fn pack_visual_effect_params(rect: api::RectF, effect: api::VisualEffect) -> [f32; 8] {
    let tint = effect.tint();
    [rect.x, rect.y, rect.w, rect.h, tint.r, tint.g, tint.b, tint.a]
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
struct RRectGpuParams {
    rect: [f32; 4],
    radii: [f32; 4],
    color: [f32; 4],
}

#[inline]
fn pack_rrect_params(rect: api::RectF, radii: [f32; 4], color: api::Color) -> RRectGpuParams {
    RRectGpuParams {
        rect: [rect.x, rect.y, rect.w, rect.h],
        radii,
        color: [color.r, color.g, color.b, color.a],
    }
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

#[doc(hidden)]
pub fn camera_blur_pass_plan(requested_sigma: f32) -> (u32, f32) {
    let sigma = if requested_sigma.is_finite() { requested_sigma.max(6.0) } else { 6.0 };
    let passes = ((sigma / 6.0).ceil() as u32).clamp(1, 4);
    (passes, (sigma / passes as f32).max(0.001))
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

#[derive(Clone, Copy)]
enum NormalizedIndexMode {
    Local,
    Rebase { vertex_base: u32 },
}

#[inline]
fn normalized_index_mode(
    source: &[u16],
    vertex_base: u32,
    vertex_count: u32,
) -> Option<NormalizedIndexMode> {
    if source.is_empty() {
        return Some(NormalizedIndexMode::Local);
    }
    if vertex_count == 0 {
        return None;
    }

    if vertex_count <= u16::MAX as u32 {
        let local_limit = vertex_count as u16;
        let local = source.iter().all(|index| *index < local_limit);
        if local {
            return Some(NormalizedIndexMode::Local);
        }
    }

    let vertex_end = vertex_base.saturating_add(vertex_count);
    for index in source.iter().copied() {
        let absolute = index as u32;
        if absolute < vertex_base || absolute >= vertex_end {
            return None;
        }
    }
    Some(NormalizedIndexMode::Rebase { vertex_base })
}

#[inline]
fn glyph_local_index(index: u16, mode: NormalizedIndexMode) -> Option<u16>
{
   match mode
   {
      NormalizedIndexMode::Local => Some(index),
      NormalizedIndexMode::Rebase { vertex_base } =>
      {
         u16::try_from(u32::from(index).checked_sub(vertex_base)?).ok()
      }
   }
}

fn append_glyph_instances(
   list_vertices: &[api::Vertex],
   list_indices: &[u16],
   run: api::GlyphRun,
   out: &mut alloc::vec::Vec<GlyphGpuInstance>,
) -> Option<usize>
{
   let vertices = list_vertices.get(
      run.vb.offset as usize..run.vb.offset as usize + run.vb.len as usize,
   )?;
   let indices = list_indices.get(
      run.ib.offset as usize..run.ib.offset as usize + run.ib.len as usize,
   )?;
   if vertices.len() % 4 != 0 || indices.len() != vertices.len() / 4 * 6
   {
      return None;
   }
   let mode = normalized_index_mode(indices, run.vb.offset, run.vb.len)?;
   let start = out.len();
   for (glyph, quad) in vertices.chunks_exact(4).enumerate()
   {
      let index_start = glyph * 6;
      let mut topology = [0_u16; 6];
      for (output, index) in topology.iter_mut().zip(
         indices[index_start..index_start + 6].iter().copied(),
      )
      {
         let local = glyph_local_index(index, mode)?;
         *output = local.checked_sub(glyph as u16 * 4)?;
      }
      let [top_left, top_right, third, fourth] = quad else
      {
         out.truncate(start);
         return None;
      };
      let (bottom_left, bottom_right) = match topology
      {
         [0, 1, 2, 2, 1, 3] => (third, fourth),
         [0, 1, 2, 0, 2, 3] => (fourth, third),
         _ =>
         {
            out.truncate(start);
            return None;
         }
      };
      if top_left.y != top_right.y
         || bottom_left.y != bottom_right.y
         || top_left.x != bottom_left.x
         || top_right.x != bottom_right.x
         || top_left.v != top_right.v
         || bottom_left.v != bottom_right.v
         || top_left.u != bottom_left.u
         || top_right.u != bottom_right.u
      {
         out.truncate(start);
         return None;
      }
      out.push(GlyphGpuInstance {
         dst: [
            top_left.x,
            top_left.y,
            top_right.x - top_left.x,
            bottom_left.y - top_left.y,
         ],
         uv: [top_left.u, top_left.v, top_right.u, bottom_left.v],
         color: [run.color.r, run.color.g, run.color.b, run.color.a],
      });
   }
   Some(out.len() - start)
}

#[inline]
fn copy_normalized_indices_for_local_vertex_span(
    source: &[u16],
    vertex_base: u32,
    vertex_count: u32,
    dst: &mut [u8],
) -> Option<usize> {
    let byte_count = source.len() * core::mem::size_of::<u16>();
    if dst.len() < byte_count {
        return None;
    }

    match normalized_index_mode(source, vertex_base, vertex_count)? {
        NormalizedIndexMode::Local => {
            let source_bytes =
                unsafe { core::slice::from_raw_parts(source.as_ptr() as *const u8, byte_count) };
            dst[..byte_count].copy_from_slice(source_bytes);
        }
        NormalizedIndexMode::Rebase { vertex_base } => {
            for (out, index) in dst[..byte_count].chunks_exact_mut(2).zip(source.iter().copied()) {
                let bytes = ((index as u32 - vertex_base) as u16).to_ne_bytes();
                out[0] = bytes[0];
                out[1] = bytes[1];
            }
        }
    }
    Some(source.len())
}

#[inline]
fn append_remapped_indices_to_span(
    source: &[u16],
    src_vertex_base: u32,
    src_vertex_count: u32,
    dst_vertex_base: u32,
    out: &mut alloc::vec::Vec<u16>,
) -> Option<usize> {
    let mode = normalized_index_mode(source, src_vertex_base, src_vertex_count)?;
    let start_len = out.len();
    out.reserve(source.len());
    for index in source.iter().copied() {
        let local = match mode {
            NormalizedIndexMode::Local => index as u32,
            NormalizedIndexMode::Rebase { vertex_base } => index as u32 - vertex_base,
        };
        let Some(dst) = dst_vertex_base.checked_add(local) else {
            out.truncate(start_len);
            return None;
        };
        if dst > u16::MAX as u32 {
            out.truncate(start_len);
            return None;
        }
        out.push(dst as u16);
    }
    Some(source.len())
}

fn append_offset_geometry_to_sublist(
    vertices: &[api::Vertex],
    indices: &[u16],
    sub: &mut api::DrawList,
    vb: api::VertexSpan,
    ib: api::IndexSpan,
    ox: f32,
    oy: f32,
) -> Option<(api::VertexSpan, api::IndexSpan)> {
    let v_count = vb.len as usize;
    let i_count = ib.len as usize;
    let Some(srcv) = vertices.get(vb.offset as usize..vb.offset as usize + v_count) else {
        return None;
    };
    let Some(srci) = indices.get(ib.offset as usize..ib.offset as usize + i_count) else {
        return None;
    };
    let Ok(new_v_off) = u32::try_from(sub.vertices.len()) else {
        return None;
    };
    let Ok(ib_offset) = u32::try_from(sub.indices.len()) else {
        return None;
    };
    for vertex in srcv {
        let mut out = *vertex;
        out.x -= ox;
        out.y -= oy;
        sub.vertices.push(out);
    }
    let Some(remapped_len) =
        append_remapped_indices_to_span(srci, vb.offset, vb.len, new_v_off, &mut sub.indices)
    else {
        let len = sub.vertices.len().saturating_sub(v_count);
        sub.vertices.truncate(len);
        return None;
    };
    Some((
        api::VertexSpan { offset: new_v_off, len: vb.len },
        api::IndexSpan { offset: ib_offset, len: remapped_len as u32 },
    ))
}

#[cfg(test)]
#[inline]
fn normalize_indices_for_local_vertex_span(
    source: &[u16],
    vertex_base: u32,
    vertex_count: u32,
) -> Option<alloc::vec::Vec<u16>> {
    match normalized_index_mode(source, vertex_base, vertex_count)? {
        NormalizedIndexMode::Local => Some(source.to_vec()),
        NormalizedIndexMode::Rebase { vertex_base } => {
            let mut rebased = alloc::vec::Vec::with_capacity(source.len());
            for index in source.iter().copied() {
                rebased.push((index as u32 - vertex_base) as u16);
            }
            Some(rebased)
        }
    }
}

#[cfg(test)]
#[inline]
fn remap_indices_to_span(
    source: &[u16],
    src_vertex_base: u32,
    src_vertex_count: u32,
    dst_vertex_base: u32,
) -> Option<alloc::vec::Vec<u16>> {
    let mut mapped = alloc::vec::Vec::new();
    append_remapped_indices_to_span(
        source,
        src_vertex_base,
        src_vertex_count,
        dst_vertex_base,
        &mut mapped,
    )?;
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
    let vdesc = api_vertex_descriptor();
    desc.set_vertex_descriptor(Some(vdesc));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_source_alpha_blend(ca);
    pipeline_state(device, "pso.solid.create", &desc)
}

#[inline]
fn api_vertex_descriptor() -> &'static VertexDescriptorRef {
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
    vdesc
}

#[inline]
fn configure_blend(
    ca: &RenderPipelineColorAttachmentDescriptorRef,
    source: MTLBlendFactor,
    destination: MTLBlendFactor,
) {
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(source);
    ca.set_source_alpha_blend_factor(source);
    ca.set_destination_rgb_blend_factor(destination);
    ca.set_destination_alpha_blend_factor(destination);
}

#[inline]
fn configure_source_alpha_blend(ca: &RenderPipelineColorAttachmentDescriptorRef) {
    configure_blend(ca, MTLBlendFactor::SourceAlpha, MTLBlendFactor::OneMinusSourceAlpha);
}

#[inline]
fn configure_layer_source_alpha_blend(ca: &RenderPipelineColorAttachmentDescriptorRef) {
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::One);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
}

#[inline]
fn configure_ui_source_alpha_blend(
    ca: &RenderPipelineColorAttachmentDescriptorRef,
    layer: bool,
) {
    if layer {
        configure_layer_source_alpha_blend(ca);
    } else {
        configure_source_alpha_blend(ca);
    }
}

#[inline]
fn configure_frame_color_attachment(
    ca: &RenderPassColorAttachmentDescriptorRef,
    texture: &TextureRef,
    initialized: bool,
) {
    ca.set_texture(Some(texture));
    ca.set_store_action(MTLStoreAction::Store);
    if initialized {
        ca.set_load_action(MTLLoadAction::Load);
    } else {
        ca.set_load_action(MTLLoadAction::Clear);
        ca.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
    }
}

#[inline]
fn configure_additive_blend(ca: &RenderPipelineColorAttachmentDescriptorRef) {
    configure_blend(ca, MTLBlendFactor::One, MTLBlendFactor::One);
}

fn build_blur_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    fragment: &str,
    create_stage: &'static str,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "blur.vertex", "v_fullscreen")?;
    let f = pipeline_function(lib, "blur.fragment", fragment)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    pipeline_state(device, create_stage, &desc)
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

fn build_bloom_composite_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "bloom_composite.vertex", "v_fullscreen")?;
    let f = pipeline_function(lib, "bloom_composite.fragment", "f_bloom_composite")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_additive_blend(ca);
    pipeline_state(device, "pso.bloom_composite.create", &desc)
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
    configure_source_alpha_blend(ca);
    pipeline_state(device, "pso.backdrop.create", &desc)
}

fn build_visual_effect_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "visual_effect.vertex", "v_backdrop")?;
    let f = pipeline_function(lib, "visual_effect.fragment", "f_visual_effect")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    pipeline_state(device, "pso.visual_effect.create", &desc)
}

fn build_image_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "image.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "image.fragment", "f_image")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer { "pso.layer_image.create" } else { "pso.image.create" };
    pipeline_state(device, stage, &desc)
}

fn build_image_single_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "image_single.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "image_single.fragment", "f_image_single")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer {
        "pso.layer_image_single.create"
    } else {
        "pso.image_single.create"
    };
    pipeline_state(device, stage, &desc)
}

fn build_image_mesh_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "image_mesh.vertex", "v_text")?;
    let f = pipeline_function(lib, "image_mesh.fragment", "f_image_mesh")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = api_vertex_descriptor();
    desc.set_vertex_descriptor(Some(vdesc));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer { "pso.layer_image_mesh.create" } else { "pso.image_mesh.create" };
    pipeline_state(device, stage, &desc)
}

fn build_rrect_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "rrect.vertex", "v_inst_rrect")?;
    let f = pipeline_function(lib, "rrect.fragment", "f_rrect")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_source_alpha_blend(ca);
    pipeline_state(device, "pso.rrect.create", &desc)
}

fn build_layer_rrect_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let vertex = pipeline_function(lib, "layer_rrect.vertex", "v_inst_rrect")?;
    let fragment = pipeline_function(lib, "layer_rrect.fragment", "f_rrect")?;
    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_vertex_function(Some(&vertex));
    descriptor.set_fragment_function(Some(&fragment));
    descriptor.set_sample_count(sample_count as u64);
    let attachment = descriptor.color_attachments().object_at(0).unwrap();
    attachment.set_pixel_format(fmt);
    configure_layer_source_alpha_blend(attachment);
    pipeline_state(device, "pso.layer_rrect.create", &descriptor)
}

fn build_nine_slice_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "nine_slice.vertex", "v_inst_nine_slice")?;
    let f = pipeline_function(lib, "nine_slice.fragment", "f_nine_slice")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer { "pso.layer_nine_slice.create" } else { "pso.nine_slice.create" };
    pipeline_state(device, stage, &desc)
}

fn build_layer_composite_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let vertex = pipeline_function(lib, "layer_composite.vertex", "v_inst_rect")?;
    let fragment = pipeline_function(lib, "layer_composite.fragment", "f_nine_slice")?;
    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_vertex_function(Some(&vertex));
    descriptor.set_fragment_function(Some(&fragment));
    descriptor.set_sample_count(sample_count as u64);
    let attachment = descriptor.color_attachments().object_at(0).unwrap();
    attachment.set_pixel_format(fmt);
    attachment.set_blending_enabled(true);
    attachment.set_rgb_blend_operation(MTLBlendOperation::Add);
    attachment.set_alpha_blend_operation(MTLBlendOperation::Add);
    attachment.set_source_rgb_blend_factor(MTLBlendFactor::One);
    attachment.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    attachment.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    attachment.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.layer_composite.create", &descriptor)
}

fn build_layer_composite_aligned_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
) -> Result<RenderPipelineState, MetalInitError> {
    let vertex = pipeline_function(lib, "layer_composite_aligned.vertex", "v_inst_rect")?;
    let fragment =
        pipeline_function(lib, "layer_composite_aligned.fragment", "f_layer_composite_aligned")?;
    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_vertex_function(Some(&vertex));
    descriptor.set_fragment_function(Some(&fragment));
    descriptor.set_sample_count(sample_count as u64);
    let attachment = descriptor.color_attachments().object_at(0).unwrap();
    attachment.set_pixel_format(fmt);
    attachment.set_blending_enabled(true);
    attachment.set_rgb_blend_operation(MTLBlendOperation::Add);
    attachment.set_alpha_blend_operation(MTLBlendOperation::Add);
    attachment.set_source_rgb_blend_factor(MTLBlendFactor::One);
    attachment.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    attachment.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    attachment.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    pipeline_state(device, "pso.layer_composite_aligned.create", &descriptor)
}

fn build_spinner_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "spinner.vertex", "v_inst_rect")?;
    let f = pipeline_function(lib, "spinner.fragment", "f_spinner")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer { "pso.layer_spinner.create" } else { "pso.spinner.create" };
    pipeline_state(device, stage, &desc)
}

fn build_text_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "text.vertex", "v_glyph")?;
    let f = pipeline_function(lib, "text.fragment", "f_glyph")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer { "pso.layer_text.create" } else { "pso.text.create" };
    pipeline_state(device, stage, &desc)
}

fn build_text_sdf_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    sample_count: u32,
    layer: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "text_sdf.vertex", "v_glyph")?;
    let f = pipeline_function(lib, "text_sdf.fragment", "f_glyph_sdf")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(sample_count as u64);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_ui_source_alpha_blend(ca, layer);
    let stage = if layer { "pso.layer_text_sdf.create" } else { "pso.text_sdf.create" };
    pipeline_state(device, stage, &desc)
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

fn build_scene3d_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    depth_only: bool,
    blend: scene3d::BlendMode3d,
    topology: scene3d::MeshTopology,
    depth_attachment: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "scene3d.vertex", "v_scene3d")?;
    let f = pipeline_function(lib, "scene3d.fragment", "f_scene3d")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    desc.set_depth_attachment_pixel_format(if depth_attachment {
        MTLPixelFormat::Depth32Float
    } else {
        MTLPixelFormat::Invalid
    });

    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float3);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(12);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));

    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    match blend {
        scene3d::BlendMode3d::Alpha => {
            configure_source_alpha_blend(ca);
        }
        scene3d::BlendMode3d::Additive => {
            configure_blend(ca, MTLBlendFactor::SourceAlpha, MTLBlendFactor::One);
        }
    }
    if depth_only {
        ca.set_write_mask(MTLColorWriteMask::empty());
    }

    let stage = match (topology, depth_only, blend) {
        (scene3d::MeshTopology::Triangles, false, scene3d::BlendMode3d::Alpha) => {
            "pso.scene3d.tri.create"
        }
        (scene3d::MeshTopology::Triangles, true, _) => "pso.scene3d.tri_depth.create",
        (scene3d::MeshTopology::Triangles, false, scene3d::BlendMode3d::Additive) => {
            "pso.scene3d.tri_add.create"
        }
        (scene3d::MeshTopology::Lines, false, scene3d::BlendMode3d::Alpha) => {
            "pso.scene3d.line.create"
        }
        (scene3d::MeshTopology::Lines, true, _) => "pso.scene3d.line_depth.create",
        (scene3d::MeshTopology::Lines, false, scene3d::BlendMode3d::Additive) => {
            "pso.scene3d.line_add.create"
        }
    };
    pipeline_state(device, stage, &desc)
}

fn build_scene3d_color_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    blend: scene3d::BlendMode3d,
    color_write: bool,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "scene3d_color.vertex", "v_scene3d_color")?;
    let f = pipeline_function(lib, "scene3d_color.fragment", "f_scene3d_color")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    desc.set_depth_attachment_pixel_format(MTLPixelFormat::Depth32Float);

    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float3);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float4);
    attrs.object_at(1).unwrap().set_offset(core::mem::size_of::<[f32; 3]>() as u64);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(core::mem::size_of::<scene3d::VertexColor3d>() as u64);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));

    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    match blend {
        scene3d::BlendMode3d::Alpha => {
            configure_source_alpha_blend(ca);
        }
        scene3d::BlendMode3d::Additive => {
            configure_blend(ca, MTLBlendFactor::SourceAlpha, MTLBlendFactor::One);
        }
    }
    if !color_write {
        ca.set_write_mask(MTLColorWriteMask::empty());
    }

    let stage = match (blend, color_write) {
        (_, false) => "pso.scene3d.color_tri_depth.create",
        (scene3d::BlendMode3d::Alpha, true) => "pso.scene3d.color_tri.create",
        (scene3d::BlendMode3d::Additive, true) => "pso.scene3d.color_tri_add.create",
    };
    pipeline_state(device, stage, &desc)
}

fn build_depth_stencil_state(
    device: &Device,
    depth_test: bool,
    depth_write: bool,
    label: &str,
) -> DepthStencilState {
    let desc = DepthStencilDescriptor::new();
    desc.set_label(label);
    desc.set_depth_compare_function(if depth_test {
        MTLCompareFunction::LessEqual
    } else {
        MTLCompareFunction::Always
    });
    desc.set_depth_write_enabled(depth_write);
    device.new_depth_stencil_state(&desc)
}

fn scene3d_primitive(topology: scene3d::MeshTopology) -> MTLPrimitiveType {
    match topology {
        scene3d::MeshTopology::Triangles => MTLPrimitiveType::Triangle,
        scene3d::MeshTopology::Lines => MTLPrimitiveType::Line,
    }
}

fn scene3d_draw_configuration(
    mesh: &Mesh3dGpu,
    instance: &scene3d::Instance3d,
    bloom_target: bool,
) -> Result<(Scene3dPipelineKind, Scene3dDepthKind, bool), api::RenderError> {
    let pipeline = match (mesh.format, mesh.topology, instance.color_write, instance.blend) {
        (MeshFormat3d::PositionColor, _, _, _) if bloom_target => {
            return Err(api::RenderError::InvalidOperation(
                "colored scene3d mesh bloom target is not supported",
            ));
        }
        (MeshFormat3d::PositionColor, scene3d::MeshTopology::Lines, _, _) => {
            return Err(api::RenderError::InvalidOperation(
                "colored scene3d mesh only supports triangles",
            ));
        }
        (MeshFormat3d::PositionColor, scene3d::MeshTopology::Triangles, false, _) => {
            Scene3dPipelineKind::ColorTriDepth
        }
        (
            MeshFormat3d::PositionColor,
            scene3d::MeshTopology::Triangles,
            true,
            scene3d::BlendMode3d::Alpha,
        ) => Scene3dPipelineKind::ColorTriAlpha,
        (
            MeshFormat3d::PositionColor,
            scene3d::MeshTopology::Triangles,
            true,
            scene3d::BlendMode3d::Additive,
        ) => Scene3dPipelineKind::ColorTriAdditive,
        (MeshFormat3d::Position, scene3d::MeshTopology::Triangles, _, _) if bloom_target => {
            Scene3dPipelineKind::PositionTriBloom
        }
        (MeshFormat3d::Position, scene3d::MeshTopology::Lines, _, _) if bloom_target => {
            Scene3dPipelineKind::PositionLineBloom
        }
        (MeshFormat3d::Position, scene3d::MeshTopology::Triangles, false, _) => {
            Scene3dPipelineKind::PositionTriDepth
        }
        (
            MeshFormat3d::Position,
            scene3d::MeshTopology::Triangles,
            true,
            scene3d::BlendMode3d::Alpha,
        ) => Scene3dPipelineKind::PositionTriAlpha,
        (
            MeshFormat3d::Position,
            scene3d::MeshTopology::Triangles,
            true,
            scene3d::BlendMode3d::Additive,
        ) => Scene3dPipelineKind::PositionTriAdditive,
        (MeshFormat3d::Position, scene3d::MeshTopology::Lines, false, _) => {
            Scene3dPipelineKind::PositionLineDepth
        }
        (
            MeshFormat3d::Position,
            scene3d::MeshTopology::Lines,
            true,
            scene3d::BlendMode3d::Alpha,
        ) => Scene3dPipelineKind::PositionLineAlpha,
        (
            MeshFormat3d::Position,
            scene3d::MeshTopology::Lines,
            true,
            scene3d::BlendMode3d::Additive,
        ) => Scene3dPipelineKind::PositionLineAdditive,
    };
    let depth = if bloom_target {
        Scene3dDepthKind::Disabled
    } else {
        match (instance.depth_test, instance.depth_write) {
            (false, false) => Scene3dDepthKind::Disabled,
            (true, false) => Scene3dDepthKind::Read,
            (true, true) => Scene3dDepthKind::Write,
            (false, true) => Scene3dDepthKind::WriteNoTest,
        }
    };
    let batchable = !bloom_target
        && mesh.opaque
        && instance.color.a >= 1.0
        && instance.color_write
        && instance.blend == scene3d::BlendMode3d::Alpha
        && instance.depth_test
        && instance.depth_write;
    Ok((pipeline, depth, batchable))
}

fn scene3d_cull_mode(cull: scene3d::CullMode3d) -> MTLCullMode {
    match cull {
        scene3d::CullMode3d::None => MTLCullMode::None,
        scene3d::CullMode3d::Front => MTLCullMode::Front,
        scene3d::CullMode3d::Back => MTLCullMode::Back,
    }
}

fn scene3d_material_id(material: scene3d::Material3d) -> u32 {
    match material {
        scene3d::Material3d::Flat => 0,
        scene3d::Material3d::NeighborhoodFill => 1,
        scene3d::Material3d::Emissive => 2,
    }
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

fn saturating_resource_bytes(dimensions: &[u64], bytes_per_element: u64) -> u64 {
    dimensions.iter().copied().fold(bytes_per_element, u64::saturating_mul)
}

struct PerFrame {
    cmd: Option<CommandBuffer>,
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
        self.reset();
        self.cmd = None;
    }
}

#[inline]
fn frame_slot_bit(slot: usize) -> u8
{
    1_u8 << slot
}

struct Ring {
    bufs: [Buffer; MAX_FRAME_RESOURCE_DEPTH],
    cap: [usize; MAX_FRAME_RESOURCE_DEPTH],
    opts: MTLResourceOptions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskVertexUploadKey {
    content_hash: u64,
    byte_len: usize,
}

struct IdMaskVertexUploadCache {
    key: IdMaskVertexUploadKey,
    buffer: Buffer,
}

impl Ring {
    fn new(device: &Device, depth: usize, initial: usize, opts: MTLResourceOptions) -> Self {
        let first = device.new_buffer(initial as u64, opts);
        let mut ring = Self {
            bufs: core::array::from_fn(|_| first.to_owned()),
            cap: [0; MAX_FRAME_RESOURCE_DEPTH],
            opts,
        };
        for slot in 0..depth
        {
            if slot > 0
            {
                ring.bufs[slot] = device.new_buffer(initial as u64, opts);
            }
            ring.cap[slot] = initial;
        }
        ring
    }
    fn ensure_capacity(&mut self, device: &Device, slot: usize, needed: usize) -> bool
    {
        if needed <= self.cap[slot]
        {
            return false;
        }
        self.grow(device, slot, needed);
        true
    }

    #[cold]
    #[inline(never)]
    fn grow(&mut self, device: &Device, slot: usize, needed: usize)
    {
        let mut new_cap = self.cap[slot] + self.cap[slot] / 2;
        if new_cap < needed
        {
            new_cap = needed;
        }
        let old = self.buffer(slot).to_owned();
        let old_cap = self.cap[slot];
        let new_buf = device.new_buffer(new_cap as u64, self.opts);
        let copy_len = old_cap.min(new_cap);
        if copy_len > 0
        {
            unsafe
            {
                core::ptr::copy_nonoverlapping(
                    old.contents() as *const u8,
                    new_buf.contents() as *mut u8,
                    copy_len,
                );
            }
        }
        if slot == 0
        {
            for alias in 1..MAX_FRAME_RESOURCE_DEPTH
            {
                if self.cap[alias] == 0
                {
                    self.bufs[alias] = new_buf.to_owned();
                }
            }
        }
        self.bufs[slot] = new_buf;
        self.cap[slot] = new_cap;
    }
    fn contents_ptr(&self, slot: usize) -> NonNull<u8> {
        let p = self.buffer(slot).contents();
        NonNull::new(p as *mut u8).expect("non-null")
    }

    #[inline]
    fn buffer(&self, slot: usize) -> &Buffer
    {
        &self.bufs[slot]
    }
}

extern crate alloc;

#[derive(Debug)]
struct LayerEntry {
    tex: Texture,
    w: u32,
    h: u32,
    generation: u64,
    prepared_key: Option<prepared::PreparedLayerKey>,
    resources: alloc::vec::Vec<api::RenderResourceDependency>,
    bytes: u64,
    last_used_frame: u64,
}

#[derive(Debug)]
struct LayerPoolEntry {
    tex: Texture,
    w: u32,
    h: u32,
    format: MTLPixelFormat,
    bytes: u64,
    recycled_frame: u64,
}

#[derive(Debug, Default)]
struct ImageArgTable
{
    handles: alloc::vec::Vec<u32>,
    offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerPlanAction {
    Inline,
    Composite,
}

#[derive(Clone, Copy, Debug)]
struct LayerPlan {
    id: u32,
    begin: usize,
    end: usize,
    rect: api::RectF,
    generation: u64,
    refresh: bool,
    action: LayerPlanAction,
}

#[derive(Clone, Copy, Debug)]
struct LayerPlanStackEntry {
    id: u32,
    begin: usize,
    rect: api::RectF,
    dirty: bool,
    unsupported: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PerfMemoryStats {
    pub logical_total_bytes: u64,
    pub logical_texture_bytes: u64,
    pub logical_buffer_bytes: u64,
    pub total_bytes: u64,
    pub draw_targets_bytes: u64,
    pub draw_target_main_bytes: u64,
    pub draw_target_msaa_bytes: u64,
    pub effect_targets_bytes: u64,
    pub effect_prepass_bytes: u64,
    pub effect_blur_chain_bytes: u64,
    pub depth_target_bytes: u64,
    pub bloom_targets_bytes: u64,
    pub id_mask_target_bytes: u64,
    pub live_camera_bytes: u64,
    pub camera_cache_bytes: u64,
    pub camera_blur_cache_bytes: u64,
    pub camera_transition_cache_bytes: u64,
    pub benchmark_camera_bytes: u64,
    pub layer_cache_bytes: u64,
    pub layer_cache_pool_bytes: u64,
    pub layer_cache_cpu_bytes: u64,
    pub image_cache_bytes: u64,
    pub scene3d_mesh_buffer_bytes: u64,
    pub id_mask_vertex_buffer_bytes: u64,
    pub prepared_cache_bytes: u64,
    pub frame_ring_buffer_bytes: u64,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub uniform_buffer_bytes: u64,
    pub argument_buffer_bytes: u64,
    pub buffer_bytes: u64,
    pub cpu_staging_bytes: u64,
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
    pub analytic_instance_bytes: u64,
    pub analytic_instance_buffer_binds: u32,
    pub analytic_instance_ring_grows: u32,
    pub scene3d_draws: u32,
    pub scene3d_instances: u32,
    pub scene3d_instance_bytes: u64,
    pub scene3d_pipeline_binds: u32,
    pub scene3d_depth_state_binds: u32,
    pub scene3d_cull_sets: u32,
    pub scene3d_mesh_buffer_binds: u32,
    pub scene3d_instance_buffer_binds: u32,
    pub scene3d_instance_ring_grows: u32,
    pub scene3d_viewport_sets: u32,
    pub effect_graph_effects: u32,
    pub effect_graph_captures: u32,
    pub effect_graph_pyramids: u32,
    pub effect_graph_pyramid_reuses: u32,
    pub effect_graph_plan_reuses: u32,
    pub effect_graph_capture_passes: u32,
    pub effect_graph_downsample_passes: u32,
    pub effect_graph_blur_horizontal_passes: u32,
    pub effect_graph_blur_vertical_passes: u32,
    pub effect_graph_composite_passes: u32,
    pub effect_graph_max_lifetime_commands: u32,
    pub effect_graph_resources: u32,
    pub effect_graph_alias_slots: u32,
    pub effect_graph_logical_bytes: u64,
    pub effect_graph_physical_bytes: u64,
    pub effect_graph_aliased_bytes: u64,
    pub blur_kernel_paired_passes: u32,
    pub blur_kernel_exact_passes: u32,
    pub blur_kernel_source_samples: u64,
    pub blur_kernel_encoded_samples: u64,
    pub blur_kernel_runtime_exp_taps: u64,
    pub blur_kernel_table_bytes: u64,
    pub glyph_instance_bytes: u64,
    pub glyph_instance_buffer_binds: u32,
    pub glyph_instances: u32,
    pub icb_cmds: u32,
    pub commands_traversed: u64,
    pub commands_copied: u64,
    pub geometry_bytes_copied: u64,
    pub chunks_reused: u64,
    pub chunks_rebuilt: u64,
    pub chunks_prepared: u64,
    pub prepared_plan_reuses: u64,
    pub backend_cache_hits: u64,
    pub backend_cache_misses: u64,
    pub damage_instances_visited: u64,
    pub damage_instances_matched: u64,
    pub damage_commands_visited: u64,
    pub damage_commands_matched: u64,
    pub damage_vertices_visited: u64,
    pub damage_query_ms: f64,
    pub layer_body_commands_scanned: u64,
    pub layer_body_commands_copied: u64,
    pub layer_texture_creates: u32,
    pub layer_cache_hits: u32,
    pub layer_cache_misses: u32,
    pub layer_cache_budget_bytes: u64,
    pub layer_cache_resident_bytes: u64,
    pub layer_cache_pool_bytes: u64,
    pub layer_cache_cpu_bytes: u64,
    pub layer_cache_oldest_last_used_frame: u64,
    pub layer_cache_pool_reuses: u64,
    pub layer_cache_evictions: u64,
    pub layer_cache_recreations: u64,
    pub layer_cache_purges: u64,
    pub layer_cache_last_purge_reason: u8,
    pub layer_offscreen_draws: u64,
    pub layer_inline_draws: u64,
    pub layer_double_render_prevented: u32,
    pub id_mask_cache_hits: u32,
    pub id_mask_cache_misses: u32,
    pub id_mask_cache_budget_bytes: u64,
    pub id_mask_cache_resident_bytes: u64,
    pub id_mask_cache_evictions: u64,
    pub id_mask_cache_entries: u32,
    pub id_mask_raster_passes: u32,
    pub id_mask_field_seed_passes: u32,
    pub id_mask_field_jump_passes: u32,
    pub id_mask_compositor_passes: u32,
    pub id_mask_target_creates: u32,
    pub id_mask_in_flight_generations: u32,
    pub id_mask_in_flight_target_bytes: u64,
    pub id_mask_target_storage_bytes: u64,
    pub id_mask_target_peak_bytes: u64,
    pub id_mask_target_reuse_blocked: u64,
    pub image_argument_encodes: u32,
    pub image_argument_binds: u32,
    pub image_argument_tables_finalized: u32,
    pub image_argument_table_reuses: u32,
    pub image_argument_bytes: u64,
    pub image_argument_buffer_grows: u32,
    pub render_passes: u32,
    pub blit_passes: u32,
    pub command_buffers: u32,
    pub texture_copies: u32,
    pub texture_copy_pixels: u64,
    pub texture_copy_bytes: u64,
    pub buffer_upload_bytes: u64,
    pub property_upload_bytes: u64,
    pub property_records_updated: u32,
    pub property_ring_bytes: u64,
    pub texture_upload_bytes: u64,
    pub shaded_damage_px: u64,
    pub cache_bytes: u64,
    pub cache_evictions: u32,
    pub wakeups: u32,
    pub skipped_submissions: u32,
    pub actual_submissions: u32,
    pub resource_creates: u32,
    pub resource_grows: u32,
    pub encode_ms: f64,
    pub gpu_frame_id: u64,
    pub gpu_ms: f64,
    pub gpu_render_ms: f64,
    pub gpu_vertex_ms: f64,
    pub gpu_fragment_ms: f64,
    pub frame_backpressure_skipped: u32,
    pub damage_px: u64,
    pub damage_pct: f32,
    pub damage_rects: u32,
    pub damage_forced_full_refreshes: u32,
    pub persistent_target_valid: u32,
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
    pub cam_gpu_render_ms: f64,
    pub cam_gpu_vertex_ms: f64,
    pub cam_gpu_fragment_ms: f64,
    pub preview_submission_depth: u32,
    pub preview_submission_skipped: u32,
    pub preview_submission_frame_age_ms: f64,
}

impl MetalRenderer {
    pub fn last_stats(&self) -> PerfStats {
        let mut stats = self.last_stats;
        self.apply_completed_gpu_stats(&mut stats);
        stats
    }

    pub fn set_damage_options(&mut self, enabled: bool, use_thresh: f32, prefilter: f32) {
        self.damage_enabled = enabled;
        self.damage_use_thresh = use_thresh.clamp(0.0, 1.0);
        self.damage_prefilter_thresh = prefilter.clamp(0.0, 1.0);
    }

    pub fn set_memory_stats_enabled_for_benchmark(&mut self, enabled: bool) {
        self.memory_stats_enabled = enabled;
        if !enabled {
            self.last_stats.memory = PerfMemoryStats::default();
        }
    }

    pub fn set_accounting_stats_enabled_for_benchmark(&mut self, enabled: bool) {
        self.accounting_stats_enabled = enabled;
        self.set_memory_stats_enabled_for_benchmark(enabled);
    }

    fn texture_allocated_bytes(tex: &TextureRef) -> u64 {
        tex.allocated_size() as u64
    }

    fn buffer_allocated_bytes(buf: &BufferRef) -> u64 {
        buf.allocated_size() as u64
    }

    fn texture_logical_bytes(tex: &TextureRef) -> u64 {
        let bytes_per_pixel = match tex.pixel_format() {
            MTLPixelFormat::R8Unorm | MTLPixelFormat::R8Uint => 1,
            MTLPixelFormat::RG8Unorm => 2,
            MTLPixelFormat::BGRA8Unorm_sRGB | MTLPixelFormat::BGRA10_XR
                | MTLPixelFormat::Depth32Float => 4,
            MTLPixelFormat::RGBA16Float | MTLPixelFormat::RGBA16Uint => 8,
            MTLPixelFormat::RGBA32Float => 16,
            _ => return Self::texture_allocated_bytes(tex),
        };
        saturating_resource_bytes(
            &[
                tex.width() as u64,
                tex.height() as u64,
                tex.depth() as u64,
                tex.array_length() as u64,
                tex.sample_count() as u64,
            ],
            bytes_per_pixel,
        )
    }

    fn push_unique_texture_logical_bytes(
        seen: &mut HashSet<usize>,
        total: &mut u64,
        tex: &TextureRef,
    ) {
        let key = tex.as_ptr() as usize;
        if seen.insert(key) {
            *total = total.saturating_add(Self::texture_logical_bytes(tex));
        }
    }

    fn push_unique_buffer_logical_bytes(
        seen: &mut HashSet<usize>,
        total: &mut u64,
        buf: &BufferRef,
    ) {
        let key = buf.as_ptr() as usize;
        if seen.insert(key) {
            *total = total.saturating_add(buf.length());
        }
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

    fn collected_memory_stats(&self) -> PerfMemoryStats {
        if self.memory_stats_enabled {
            self.memory_stats()
        } else {
            PerfMemoryStats::default()
        }
    }

    fn memory_stats(&self) -> PerfMemoryStats {
        let mut seen = self.memory_texture_seen.borrow_mut();
        seen.clear();
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
            self.half_tex
                .iter()
                .map(|tex| tex.as_ref())
                .chain(self.quarter_tex.iter().map(|tex| tex.as_ref()))
                .chain(self.quarter_tmp_tex.iter().map(|tex| tex.as_ref()))
                .chain(self.eighth_tex.iter().map(|tex| tex.as_ref()))
                .chain(self.eighth_tmp_tex.iter().map(|tex| tex.as_ref())),
        );
        let effect_targets_bytes = effect_prepass_bytes.saturating_add(effect_blur_chain_bytes);
        let depth_target_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.depth_tex.iter().map(|tex| tex.as_ref()),
        );
        let bloom_targets_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.scene3d_bloom_tex
                .iter()
                .map(|tex| tex.as_ref())
                .chain(self.scene3d_bloom_tmp_tex.iter().map(|tex| tex.as_ref())),
        );
        let retained_id_mask_target_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.id_mask_snapshot_target.iter()
                .chain(self.id_mask_field_cache.iter().map(|entry| &entry.targets))
                .flat_map(|targets| {
                    [Some(targets.city.as_ref()), Some(targets.neighborhood.as_ref())]
                        .into_iter()
                        .chain(targets.field_texture_refs())
                        .flatten()
                }),
        );
        let id_mask_target_bytes = retained_id_mask_target_bytes
            .max(self.id_mask_target_storage().2);
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
            self.layers
                .values()
                .map(|entry| entry.tex.as_ref())
                .chain(self.layer_pool.iter().map(|entry| entry.tex.as_ref())),
        );
        let layer_cache_pool_bytes = self.layer_cache_pool_bytes;
        let layer_cache_cpu_bytes = self.layer_cache_cpu_bytes();
        let image_cache_bytes = Self::unique_texture_category_bytes(
            &mut seen,
            self.images.values().map(|tex| tex.as_ref()),
        );
        drop(seen);
        let mut buffer_seen = self.memory_buffer_seen.borrow_mut();
        buffer_seen.clear();
        let vertex_buffer_bytes = Self::unique_buffer_category_bytes(
            &mut buffer_seen,
            self.vb.bufs.iter().take(self.frames.len()).map(|buf| buf.as_ref()),
        );
        let index_buffer_bytes = Self::unique_buffer_category_bytes(
            &mut buffer_seen,
            self.ib.bufs.iter().take(self.frames.len()).map(|buf| buf.as_ref()),
        );
        let uniform_buffer_bytes = Self::unique_buffer_category_bytes(
            &mut buffer_seen,
            self.ub.bufs.iter().take(self.frames.len()).map(|buf| buf.as_ref())
                .chain(self.property_ring.bufs.iter().take(self.frames.len()).map(|buf| buf.as_ref())),
        );
        let frame_ring_buffer_bytes = vertex_buffer_bytes
            .saturating_add(index_buffer_bytes)
            .saturating_add(uniform_buffer_bytes);
        let argument_buffer_bytes = Self::unique_buffer_category_bytes(
            &mut buffer_seen,
            self.img_arg_bufs
                .iter()
                .flat_map(|buffers| {
                    buffers.bufs.iter().take(self.frames.len()).map(|buffer| buffer.as_ref())
                }),
        );
        let id_mask_vertex_buffer_bytes = Self::unique_buffer_category_bytes(
            &mut buffer_seen,
            self.id_mask_vertex_caches.iter().map(|cache| cache.buffer.as_ref()),
        );
        let scene3d_mesh_buffer_bytes = Self::unique_buffer_category_bytes(
            &mut buffer_seen,
            self.meshes_3d
                .values()
                .flat_map(|mesh| [mesh.vb.as_ref(), mesh.ib.as_ref()]),
        );
        let prepared_cache_bytes = self.prepared_chunks.resident_bytes();
        let buffer_bytes = frame_ring_buffer_bytes
            .saturating_add(argument_buffer_bytes)
            .saturating_add(id_mask_vertex_buffer_bytes)
            .saturating_add(scene3d_mesh_buffer_bytes)
            .saturating_add(prepared_cache_bytes);
        drop(buffer_seen);
        let total_bytes = draw_targets_bytes
            .saturating_add(effect_targets_bytes)
            .saturating_add(depth_target_bytes)
            .saturating_add(bloom_targets_bytes)
            .saturating_add(id_mask_target_bytes)
            .saturating_add(live_camera_bytes)
            .saturating_add(camera_cache_bytes)
            .saturating_add(benchmark_camera_bytes)
            .saturating_add(layer_cache_bytes)
            .saturating_add(image_cache_bytes)
            .saturating_add(buffer_bytes);
        let mut logical_seen = self.memory_texture_seen.borrow_mut();
        logical_seen.clear();
        let mut logical_total_bytes = 0;
        for texture in self
            .target_tex
            .iter()
            .chain(self.target_msaa_tex.iter())
            .chain(self.depth_tex.iter())
            .chain(self.prepass_tex.iter())
            .chain(self.half_tex.iter())
            .chain(self.quarter_tex.iter())
            .chain(self.quarter_tmp_tex.iter())
            .chain(self.eighth_tex.iter())
            .chain(self.eighth_tmp_tex.iter())
            .chain(self.scene3d_bloom_tex.iter())
            .chain(self.scene3d_bloom_tmp_tex.iter())
            .chain(self.cam_blur_tex.iter())
            .chain(self.cam_xfade_prev_tex.iter())
            .chain(self.bench_cam_y_tex.iter())
            .chain(self.bench_cam_uv_tex.iter())
            .chain(self.bench_cam_bgra_tex.iter())
            .chain(self.layers.values().map(|entry| &entry.tex))
            .chain(self.layer_pool.iter().map(|entry| &entry.tex))
            .chain(self.images.values())
        {
            Self::push_unique_texture_logical_bytes(
                &mut logical_seen,
                &mut logical_total_bytes,
                texture,
            );
        }
        for targets in self.id_mask_snapshot_target.iter() {
            for texture in [Some(targets.city.as_ref()), Some(targets.neighborhood.as_ref())]
                .into_iter()
                .chain(targets.field_texture_refs())
                .flatten()
            {
                Self::push_unique_texture_logical_bytes(
                    &mut logical_seen,
                    &mut logical_total_bytes,
                    texture,
                );
            }
        }
        for entry in &self.id_mask_field_cache {
            for texture in [
                Some(entry.targets.city.as_ref()),
                Some(entry.targets.neighborhood.as_ref()),
            ]
            .into_iter()
            .chain(entry.targets.field_texture_refs())
            .flatten()
            {
                Self::push_unique_texture_logical_bytes(
                    &mut logical_seen,
                    &mut logical_total_bytes,
                    texture,
                );
            }
        }
        if let Some(frame) = &self.current_live_camera_frame {
            let y_tex = unsafe { TextureRef::from_ptr(frame.y_tex as *mut MTLTexture) };
            let uv_tex = unsafe { TextureRef::from_ptr(frame.uv_tex as *mut MTLTexture) };
            Self::push_unique_texture_logical_bytes(
                &mut logical_seen,
                &mut logical_total_bytes,
                y_tex,
            );
            Self::push_unique_texture_logical_bytes(
                &mut logical_seen,
                &mut logical_total_bytes,
                uv_tex,
            );
        }
        let logical_texture_bytes = logical_total_bytes;
        drop(logical_seen);
        let mut logical_buffer_seen = self.memory_buffer_seen.borrow_mut();
        logical_buffer_seen.clear();
        for buffer in self
            .vb
            .bufs
            .iter()
            .take(self.frames.len())
            .chain(self.ib.bufs.iter().take(self.frames.len()))
            .chain(self.ub.bufs.iter().take(self.frames.len()))
            .chain(self.property_ring.bufs.iter().take(self.frames.len()))
            .chain(
                self.img_arg_bufs
                    .iter()
                    .flat_map(|buffers| buffers.bufs.iter().take(self.frames.len())),
            )
            .chain(self.id_mask_vertex_caches.iter().map(|cache| &cache.buffer))
            .chain(self.meshes_3d.values().flat_map(|mesh| [&mesh.vb, &mesh.ib]))
        {
            Self::push_unique_buffer_logical_bytes(
                &mut logical_buffer_seen,
                &mut logical_total_bytes,
                buffer,
            );
        }
        let logical_buffer_bytes = logical_total_bytes.saturating_sub(logical_texture_bytes);
        let logical_buffer_bytes = logical_buffer_bytes
            .saturating_add(self.prepared_chunks.logical_resident_bytes());
        let logical_total_bytes = logical_texture_bytes.saturating_add(logical_buffer_bytes);
        drop(logical_buffer_seen);
        let cpu_staging_bytes = (self.image_vbuf.capacity() as u64)
            .saturating_mul(core::mem::size_of::<f32>() as u64)
            .saturating_add(
                (self.image_fbuf.capacity() as u64)
                    .saturating_mul(core::mem::size_of::<ImageGpuParams>() as u64),
            )
            .saturating_add(
                (self.effect_fbuf.capacity() as u64)
                    .saturating_mul(core::mem::size_of::<f32>() as u64),
            )
            .saturating_add(
                (self.scene3d_draws.capacity() as u64)
                    .saturating_mul(core::mem::size_of::<Scene3dDraw>() as u64),
            );
        PerfMemoryStats {
            logical_total_bytes,
            logical_texture_bytes,
            logical_buffer_bytes,
            total_bytes,
            draw_targets_bytes,
            draw_target_main_bytes,
            draw_target_msaa_bytes,
            effect_targets_bytes,
            effect_prepass_bytes,
            effect_blur_chain_bytes,
            depth_target_bytes,
            bloom_targets_bytes,
            id_mask_target_bytes,
            live_camera_bytes,
            camera_cache_bytes,
            camera_blur_cache_bytes,
            camera_transition_cache_bytes,
            benchmark_camera_bytes,
            layer_cache_bytes,
            layer_cache_pool_bytes,
            layer_cache_cpu_bytes,
            image_cache_bytes,
            scene3d_mesh_buffer_bytes,
            id_mask_vertex_buffer_bytes,
            prepared_cache_bytes,
            frame_ring_buffer_bytes,
            vertex_buffer_bytes,
            index_buffer_bytes,
            uniform_buffer_bytes,
            argument_buffer_bytes,
            buffer_bytes,
            cpu_staging_bytes,
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
    fn auxiliary_textures_do_not_force_persistent_final_color()
    {
        let direct = final_target_plan(1, false, false, true, true);
        assert!(direct.needs_auxiliary_textures);
        assert!(!direct.needs_persistent_final_target);
        assert!(direct.direct_present);

        for persistent in [
            final_target_plan(4, false, false, true, true),
            final_target_plan(1, true, false, true, true),
            final_target_plan(1, false, true, true, true),
            final_target_plan(1, false, false, true, false),
        ] {
            assert!(persistent.needs_auxiliary_textures);
            assert!(persistent.needs_persistent_final_target);
            assert!(!persistent.direct_present);
        }
    }

    #[test]
    fn paired_blur_kernel_preserves_normalized_gaussian_weights()
    {
        let sigma = 8.0;
        let radius = 24.0;
        let (kernel, pair_count) = paired_blur_kernel(sigma, radius, [1.0, 0.0])
            .expect("paired wide kernel");
        assert_eq!(pair_count, 12);
        assert_eq!(kernel.len(), 30);
        assert_eq!(&kernel[..4], &[1.0, 0.0, sigma, radius]);

        let mut norm = 1.0f32;
        for tap in 1..=24
        {
            let x = tap as f32 / sigma;
            norm += 2.0 * (-0.5 * x * x).exp();
        }
        let mut paired_norm = kernel[4];
        for (pair_index, pair) in kernel[6..].chunks_exact(2).enumerate()
        {
            let [offset, weight] = [pair[0], pair[1]];
            let first_tap = (pair_index * 2 + 1) as f32;
            let fraction = offset - first_tap;
            let first_weight = weight * (1.0 - fraction);
            let second_weight = weight * fraction;
            let first_x = first_tap / sigma;
            let second_x = (first_tap + 1.0) / sigma;
            assert!((first_weight - (-0.5 * first_x * first_x).exp() / norm).abs() < 1.0e-6);
            assert!((second_weight - (-0.5 * second_x * second_x).exp() / norm).abs() < 1.0e-6);
            paired_norm += 2.0 * weight;
        }
        assert!((paired_norm - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn paired_blur_kernel_keeps_subthreshold_and_non_bucket_inputs_exact()
    {
        assert!(paired_blur_kernel(1.5, 5.0, [1.0, 0.0]).is_none());
        assert!(paired_blur_kernel(4.01, 13.0, [1.0, 0.0]).is_none());
        assert!(paired_blur_kernel(8.0, 23.0, [1.0, 0.0]).is_none());
        assert!(paired_blur_kernel(8.0, 24.0, [0.5, 0.5]).is_none());
        assert!(paired_blur_kernel(f32::NAN, 24.0, [1.0, 0.0]).is_none());
        assert!(paired_blur_kernel(f32::INFINITY, 24.0, [1.0, 0.0]).is_none());
    }

    fn glyph_test_run(vertex_len: u32, index_len: u32, sdf: bool) -> api::GlyphRun
    {
        api::GlyphRun {
            atlas: api::ImageHandle(7),
            atlas_revision: 1,
            vb: api::VertexSpan { offset: 0, len: vertex_len },
            ib: api::IndexSpan { offset: 0, len: index_len },
            sdf,
            color: api::Color::rgba(0.25, 0.5, 0.75, 1.0),
        }
    }

    #[test]
    fn glyph_instance_abi_is_compact_and_converts_both_supported_quad_topologies()
    {
        assert_eq!(core::mem::size_of::<GlyphGpuInstance>(), 48);
        assert_eq!(core::mem::align_of::<GlyphGpuInstance>(), 4);
        let vertices = [
            api::Vertex { x: 2.0, y: 3.0, u: 0.1, v: 0.2, rgba: 0 },
            api::Vertex { x: 7.0, y: 3.0, u: 0.6, v: 0.2, rgba: 0 },
            api::Vertex { x: 2.0, y: 11.0, u: 0.1, v: 0.9, rgba: 0 },
            api::Vertex { x: 7.0, y: 11.0, u: 0.6, v: 0.9, rgba: 0 },
        ];
        let mut instances = Vec::new();
        assert_eq!(
            append_glyph_instances(
                &vertices,
                &[0, 1, 2, 2, 1, 3],
                glyph_test_run(4, 6, true),
                &mut instances,
            ),
            Some(1),
        );
        assert_eq!(instances[0], GlyphGpuInstance {
            dst: [2.0, 3.0, 5.0, 8.0],
            uv: [0.1, 0.2, 0.6, 0.9],
            color: [0.25, 0.5, 0.75, 1.0],
        });

        let conventional = [vertices[0], vertices[1], vertices[3], vertices[2]];
        instances.clear();
        assert_eq!(
            append_glyph_instances(
                &conventional,
                &[0, 1, 2, 0, 2, 3],
                glyph_test_run(4, 6, false),
                &mut instances,
            ),
            Some(1),
        );
        assert_eq!(instances[0].dst, [2.0, 3.0, 5.0, 8.0]);
        assert_eq!(instances[0].uv, [0.1, 0.2, 0.6, 0.9]);
        assert_eq!(instances[0].color, [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn malformed_glyph_geometry_does_not_partially_append_instances()
    {
        let vertices = [
            api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 1.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 0.0, y: 1.0, u: 0.0, v: 1.0, rgba: 0 },
            api::Vertex { x: 1.0, y: 1.0, u: 1.0, v: 1.0, rgba: 0 },
        ];
        let sentinel = GlyphGpuInstance {
            dst: [9.0; 4],
            uv: [8.0; 4],
            color: [7.0; 4],
        };
        let mut instances = vec![sentinel];
        assert_eq!(
            append_glyph_instances(
                &vertices,
                &[0, 1, 2, 0, 3, 2],
                glyph_test_run(4, 6, false),
                &mut instances,
            ),
            None,
        );
        assert_eq!(instances, [sentinel]);
    }

    #[test]
    fn gpu_param_layouts_match_metal_contracts() {
        use core::mem::{align_of, size_of};

        assert_eq!(align_of::<RRectGpuParams>(), 16);
        assert_eq!(size_of::<RRectGpuParams>(), 48);

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
    fn analytic_instance_pair_layout_supports_large_aligned_runs() {
        let (vertex_offset, fragment_offset, end) =
            analytic_instance_pair_layout::<[f32; 4], ImageGpuParams>(7, 10_000);
        assert_eq!(vertex_offset, 16);
        assert_eq!(fragment_offset, 160_016);
        assert_eq!(end, 640_016);
        assert_eq!(vertex_offset % 16, 0);
        assert_eq!(fragment_offset % core::mem::align_of::<ImageGpuParams>(), 0);
    }

    #[test]
    fn dark_popup_visual_effect_packs_single_tint_material() {
        let params = pack_visual_effect_params(
            api::RectF::new(0.0, 0.0, 402.0, 874.0),
            api::VisualEffect::DarkPopup {
                blur_intensity: 1.0,
                tint: api::Color::rgba(1.0, 0.25, 0.0, 0.90),
            },
        );

        assert_eq!(params, [0.0, 0.0, 402.0, 874.0, 1.0, 0.25, 0.0, 0.90]);
    }

    #[test]
    fn dark_popup_visual_effect_intensity_drives_composite_blur_plan() {
        let low = visual_effect_blur_plan(api::VisualEffect::DarkPopup {
            blur_intensity: 0.5,
            tint: api::Color::rgba(1.0, 1.0, 1.0, 0.9),
        });
        let high = visual_effect_blur_plan(api::VisualEffect::DarkPopup {
            blur_intensity: 1.0,
            tint: api::Color::rgba(1.0, 1.0, 1.0, 0.9),
        });
        let off = visual_effect_blur_plan(api::VisualEffect::DarkPopup {
            blur_intensity: f32::NAN,
            tint: api::Color::rgba(1.0, 1.0, 1.0, 0.9),
        });

        assert_eq!(low.downsample_divisor, 4);
        assert_eq!(low.sigma_dp, 36.0);
        assert_eq!(low.pass_scale, 4.0);
        assert_eq!(low.pass_sigma, 9.0);
        assert_eq!(low.pass_radius, 27.0);
        assert_eq!(high.downsample_divisor, 8);
        assert_eq!(high.sigma_dp, 72.0);
        assert_eq!(high.pass_scale, 8.0);
        assert_eq!(off, VisualEffectBlurPlan::OFF);
    }

    #[test]
    fn effect_target_plan_matches_declared_pass_resolution() {
        assert_eq!(
            EffectTargetPlan::for_effects(0.0, VisualEffectBlurPlan::OFF),
            EffectTargetPlan::Prepass,
        );
        assert_eq!(
            EffectTargetPlan::for_effects(8.0, VisualEffectBlurPlan::OFF),
            EffectTargetPlan::Quarter,
        );
        let low = visual_effect_blur_plan(api::VisualEffect::DarkPopup {
            blur_intensity: 0.5,
            tint: api::Color::rgba(1.0, 1.0, 1.0, 0.9),
        });
        let high = visual_effect_blur_plan(api::VisualEffect::DarkPopup {
            blur_intensity: 1.0,
            tint: api::Color::rgba(1.0, 1.0, 1.0, 0.9),
        });
        assert_eq!(EffectTargetPlan::for_effects(low.sigma_dp, low), EffectTargetPlan::Quarter);
        assert_eq!(EffectTargetPlan::for_effects(high.sigma_dp, high), EffectTargetPlan::Eighth);
    }

    #[test]
    fn metal_effect_graph_reuses_only_compatible_snapshot_epochs() {
        let backdrop = |x, sigma| api::DrawCmd::Backdrop {
            rect: api::RectF::new(x, 20.0, 24.0, 24.0),
            sigma,
            tint: api::Color::rgba(1.0, 1.0, 1.0, 0.2),
            alpha: 1.0,
        };
        let items = [
            backdrop(20.0, 12.0),
            backdrop(140.0, 12.0),
            api::DrawCmd::RRect {
                rect: api::RectF::new(0.0, 0.0, 8.0, 8.0),
                radii: [2.0; 4],
                color: api::Color::rgba(1.0, 0.0, 0.0, 1.0),
            },
            backdrop(220.0, 12.0),
            backdrop(360.0, 24.0),
        ];
        let mut events = alloc::vec::Vec::new();
        let mut plan = api::EffectGraphPlan::default();
        build_metal_effect_graph(
            &mut events,
            &mut plan,
            DrawListView { items: &items, vertices: &[], indices: &[] },
            500,
            200,
            1.0,
            1,
            4,
        );
        assert_eq!(plan.effects().len(), 4);
        assert_eq!(plan.captures().len(), 2);
        assert_eq!(plan.pyramids().len(), 3);
        assert_eq!(plan.stats().pyramid_reuses, 1);
        assert_eq!(plan.effects()[0].capture, plan.effects()[1].capture);
        assert_ne!(plan.effects()[1].capture, plan.effects()[2].capture);
        assert_ne!(plan.effects()[2].pyramid, plan.effects()[3].pyramid);
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
    fn simulator_defaults_disable_layer_cache() {
        assert!(!layer_cache_enabled_default());
    }

    #[cfg(all(target_os = "ios", not(target_abi = "sim")))]
    #[test]
    fn ios_device_defaults_enable_layer_cache_without_simulator_udid() {
        if std::env::var_os("SIMULATOR_UDID").is_some() {
            return;
        }
        assert!(layer_cache_enabled_default());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_defaults_keep_layer_cache() {
        assert!(layer_cache_enabled_default());
    }

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn ring_resizes_buffers() {
        let Some(device) = Device::system_default() else { return };
        let mut ring = Ring::new(&device, 3, 128, MTLResourceOptions::StorageModeShared);
        let initial = ring.cap[0];
        let initial_buffer = ring.buffer(0).as_ptr();
        assert!((3..MAX_FRAME_RESOURCE_DEPTH)
            .all(|slot| ring.buffer(slot).as_ptr() == initial_buffer));
        ring.ensure_capacity(&device, 0, initial * 4);
        let grown_buffer = ring.buffer(0).as_ptr();
        assert_ne!(grown_buffer, initial_buffer);
        assert!(ring.cap[0] >= initial * 4);
        assert!((3..MAX_FRAME_RESOURCE_DEPTH)
            .all(|slot| ring.buffer(slot).as_ptr() == grown_buffer));
        assert!(!ring.contents_ptr(0).as_ptr().is_null());
    }

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn ring_resizes_preserve_buffer_prefix_data() {
        let Some(device) = Device::system_default() else { return };
        let mut ring = Ring::new(&device, 3, 64, MTLResourceOptions::StorageModeShared);
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
