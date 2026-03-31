//! Oxide iOS host static library
//!
//! Exposes `rust_entry()` for the Xcode app's `main.m` to call.
//! On iOS, this calls into an Objective-C shim that starts UIApplication.
#![allow(clippy::all, clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

#[cfg(target_os = "ios")]
use core::time::Duration;
use oxide_networking::ReachabilityManager;
#[cfg(target_os = "ios")]
use oxide_networking::{
    HandshakeResponse, PacketKind, QuicSessionManager, ReachabilitySubscription, TimeSyncSample,
};
use oxide_perf_runner as perf_runner;
use oxide_permissions::{PermissionManager, PermissionState, PermissionSubscription, SensorBridge};
#[cfg(target_os = "ios")]
use oxide_platform_api::{
    Bluetooth, BluetoothEvent, CameraManager, LocationOptions, LocationService, MotionService,
    PermissionDomain, PermissionStatus, Permissions, PlatformError, PushManager, PushProvider,
    PushToken, TimeService,
};
use oxide_renderer_api as gfx_api;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_telemetry::{
    MemoryPressureLevel, TelemetryAction, TelemetryCommandReason, TelemetryHub, TelemetryOperations,
};
use oxide_test_scenes as test_scenes;
use oxide_text as text;
use oxide_timing as timing;
use oxide_ui_core as ui;
use std::{fs::File, io::Write, sync::Arc};

#[cfg(target_os = "ios")]
use std::sync::Weak;

#[cfg(target_os = "ios")]
use oxide_platform_ios::{
    bluetooth_with_restoration, IosBluetooth, IosCameraManager, IosLocation, IosMotion,
    IosPermissions, IosPushManager, IosReachability,
};

#[cfg(target_os = "ios")]
extern "C" {
    // Implemented in src/ios/app.m
    fn oxide_host_start(
        argc: ::core::ffi::c_int,
        argv: *mut *mut ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    fn oxide_host_perf_signpost_begin(ptr: *const ::core::ffi::c_char, len: usize) -> u64;
    fn oxide_host_perf_signpost_end(ptr: *const ::core::ffi::c_char, len: usize, signpost_id: u64);
    fn oxide_cam_start_default() -> ::libc::c_int;
    fn oxide_cam_start_default_preview_only() -> ::libc::c_int;
    fn oxide_cam_stop();
    fn oxide_cam_set_preview_pixel_format(format: i32) -> ::libc::c_int;
}

#[cfg(target_os = "ios")]
pub struct IosTime;

#[inline]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn camera_perf_trace_signposts_enabled() -> bool {
    #[cfg(target_os = "ios")]
    {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| ios_env_flag("OXIDE_PERF_CAMERA_TRACE_PHASES"))
    }
    #[cfg(not(target_os = "ios"))]
    {
        false
    }
}

#[inline]
fn with_perf_signpost<T>(_name: &str, body: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "ios")]
    {
        if !camera_perf_trace_signposts_enabled() {
            return body();
        }
        let signpost_id = unsafe {
            oxide_host_perf_signpost_begin(
                _name.as_ptr().cast::<::core::ffi::c_char>(),
                _name.len(),
            )
        };
        let result = body();
        unsafe {
            oxide_host_perf_signpost_end(
                _name.as_ptr().cast::<::core::ffi::c_char>(),
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

#[inline]
fn benchmark_camera_fast_path_active(app: &AppState) -> bool {
    app.benchmark_mode && app.benchmark_scene_index == benchmark_camera_scene_index()
}

#[inline]
fn benchmark_camera_scene_index() -> u32 {
    test_scenes::SceneKind::Camera as u32
}

#[cfg(target_os = "ios")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct OxideCamPerfSnapshot {
    capture_total_ms: f32,
    capture_sample_setup_ms: f32,
    capture_lock_ms: f32,
    capture_texture_bridge_ms: f32,
    capture_publish_ms: f32,
    capture_publish_lock_ms: f32,
    capture_publish_texture_refs_ms: f32,
    capture_publish_pixel_buffer_ms: f32,
    capture_frame_delivery_ms: f32,
    sample_delivery_pool_bytes: u64,
    sample_delivery_pool_surfaces: u32,
    active_sample_surface_bytes: u64,
    active_sample_surface_surfaces: u32,
    active_sample_buffers: u32,
    peak_active_sample_surface_bytes: u64,
    peak_active_sample_surface_surfaces: u32,
    peak_active_sample_buffers: u32,
    sample_delivery_total_samples: u32,
    sample_delivery_reused_frames: u32,
    sample_delivery_reused_surfaces: u32,
    sample_delivery_max_reuse_gap_frames: u32,
    retained_sample_surface_bytes: u64,
    retained_sample_surface_surfaces: u32,
    retained_published_slot_surface_bytes: u64,
    retained_published_slot_surfaces: u32,
    retained_latest_pixel_buffer_surface_bytes: u64,
    retained_latest_pixel_buffer_surface_surfaces: u32,
    latest_published_generation: u64,
    latest_published_timestamp_ns: u64,
}

#[cfg(target_os = "ios")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct OxideCamContractSnapshot {
    active_width: u32,
    active_height: u32,
    active_fps: f32,
    video_range: u32,
    color_space: u32,
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    fn oxide_cam_get_perf_snapshot(out: *mut OxideCamPerfSnapshot) -> ::libc::c_int;
    fn oxide_cam_reset_perf_counters() -> ::libc::c_int;
    fn oxide_cam_get_contract_snapshot(out: *mut OxideCamContractSnapshot) -> ::libc::c_int;
}

#[inline]
fn apply_camera_stage_perf(stats: &mut StatsSnapshot, perf_stats: &metal::PerfStats) {
    stats.cam_poll_submissions_ms = perf_stats.cam_poll_submissions_ms as f32;
    stats.cam_fetch_ms = perf_stats.cam_fetch_ms as f32;
    stats.cam_setup_ms = perf_stats.cam_setup_ms as f32;
    stats.cam_encode_quad_ms = perf_stats.cam_encode_quad_ms as f32;
    stats.cam_command_buffer_ms = perf_stats.cam_command_buffer_ms as f32;
    stats.cam_encoder_ms = perf_stats.cam_encoder_ms as f32;
    stats.cam_encode_bind_ms = perf_stats.cam_encode_bind_ms as f32;
    stats.cam_encode_draw_ms = perf_stats.cam_encode_draw_ms as f32;
    stats.cam_end_encoding_ms = perf_stats.cam_end_encoding_ms as f32;
    stats.cam_present_ms = perf_stats.cam_present_ms as f32;
    stats.cam_commit_ms = perf_stats.cam_commit_ms as f32;
    stats.cam_gpu_ms = perf_stats.cam_gpu_ms as f32;
    stats.renderer_memory_total_bytes = perf_stats.memory.total_bytes;
    stats.renderer_memory_draw_targets_bytes = perf_stats.memory.draw_targets_bytes;
    stats.renderer_memory_draw_target_main_bytes = perf_stats.memory.draw_target_main_bytes;
    stats.renderer_memory_draw_target_msaa_bytes = perf_stats.memory.draw_target_msaa_bytes;
    stats.renderer_memory_effect_targets_bytes = perf_stats.memory.effect_targets_bytes;
    stats.renderer_memory_effect_prepass_bytes = perf_stats.memory.effect_prepass_bytes;
    stats.renderer_memory_effect_blur_chain_bytes = perf_stats.memory.effect_blur_chain_bytes;
    stats.renderer_memory_live_camera_bytes = perf_stats.memory.live_camera_bytes;
    stats.renderer_memory_camera_cache_bytes = perf_stats.memory.camera_cache_bytes;
    stats.renderer_memory_camera_blur_cache_bytes = perf_stats.memory.camera_blur_cache_bytes;
    stats.renderer_memory_camera_transition_cache_bytes =
        perf_stats.memory.camera_transition_cache_bytes;
    stats.renderer_memory_benchmark_camera_bytes = perf_stats.memory.benchmark_camera_bytes;
    stats.renderer_memory_layer_cache_bytes = perf_stats.memory.layer_cache_bytes;
    stats.renderer_memory_image_cache_bytes = perf_stats.memory.image_cache_bytes;
    stats.renderer_memory_buffer_bytes = perf_stats.memory.buffer_bytes;
    stats.renderer_pending_command_buffers = perf_stats.memory.pending_command_buffers;
    stats.renderer_pending_present_drawables = perf_stats.memory.pending_present_drawables;
    stats.renderer_pending_present_textures = perf_stats.memory.pending_present_textures;
    stats.renderer_preview_submission_depth = perf_stats.preview_submission_depth;
    stats.renderer_preview_submission_skipped = perf_stats.preview_submission_skipped;
    stats.renderer_preview_submission_frame_age_ms =
        perf_stats.preview_submission_frame_age_ms as f32;
}

#[inline]
fn apply_camera_capture_perf(_stats: &mut StatsSnapshot) {
    #[cfg(target_os = "ios")]
    {
        let mut snap = OxideCamPerfSnapshot::default();
        if unsafe { oxide_cam_get_perf_snapshot(&mut snap) } == 1 {
            _stats.cam_capture_total_ms = snap.capture_total_ms;
            _stats.cam_capture_sample_setup_ms = snap.capture_sample_setup_ms;
            _stats.cam_capture_lock_ms = snap.capture_lock_ms;
            _stats.cam_capture_texture_bridge_ms = snap.capture_texture_bridge_ms;
            _stats.cam_capture_publish_ms = snap.capture_publish_ms;
            _stats.cam_capture_publish_lock_ms = snap.capture_publish_lock_ms;
            _stats.cam_capture_publish_texture_refs_ms = snap.capture_publish_texture_refs_ms;
            _stats.cam_capture_publish_pixel_buffer_ms = snap.capture_publish_pixel_buffer_ms;
            _stats.cam_capture_frame_delivery_ms = snap.capture_frame_delivery_ms;
            _stats.cam_sample_delivery_pool_bytes = snap.sample_delivery_pool_bytes;
            _stats.cam_sample_delivery_pool_surfaces = snap.sample_delivery_pool_surfaces;
            _stats.cam_active_sample_surface_bytes = snap.active_sample_surface_bytes;
            _stats.cam_active_sample_surface_surfaces = snap.active_sample_surface_surfaces;
            _stats.cam_active_sample_buffers = snap.active_sample_buffers;
            _stats.cam_peak_active_sample_surface_bytes = snap.peak_active_sample_surface_bytes;
            _stats.cam_peak_active_sample_surface_surfaces =
                snap.peak_active_sample_surface_surfaces;
            _stats.cam_peak_active_sample_buffers = snap.peak_active_sample_buffers;
            _stats.cam_sample_delivery_total_samples = snap.sample_delivery_total_samples;
            _stats.cam_sample_delivery_reused_frames = snap.sample_delivery_reused_frames;
            _stats.cam_sample_delivery_reused_surfaces = snap.sample_delivery_reused_surfaces;
            _stats.cam_sample_delivery_max_reuse_gap_frames =
                snap.sample_delivery_max_reuse_gap_frames;
            _stats.cam_retained_sample_surface_bytes = snap.retained_sample_surface_bytes;
            _stats.cam_retained_sample_surface_surfaces = snap.retained_sample_surface_surfaces;
            _stats.cam_retained_published_slot_surface_bytes =
                snap.retained_published_slot_surface_bytes;
            _stats.cam_retained_published_slot_surfaces = snap.retained_published_slot_surfaces;
            _stats.cam_retained_latest_pixel_buffer_surface_bytes =
                snap.retained_latest_pixel_buffer_surface_bytes;
            _stats.cam_retained_latest_pixel_buffer_surface_surfaces =
                snap.retained_latest_pixel_buffer_surface_surfaces;
            _stats.cam_latest_published_generation = snap.latest_published_generation;
            _stats.cam_latest_published_timestamp_ns = snap.latest_published_timestamp_ns;
        }
    }
}

#[doc(hidden)]
pub fn merge_camera_contract_fields(
    fallback_width: u32,
    fallback_height: u32,
    fallback_fps: f32,
    fallback_video_range: u8,
    fallback_color_space: u8,
    active_width: u32,
    active_height: u32,
    active_fps: f32,
    video_range: u32,
    color_space: u32,
) -> (u32, u32, f32, u8, u8) {
    let width = if active_width > 0 { active_width } else { fallback_width };
    let height = if active_height > 0 { active_height } else { fallback_height };
    let fps = if active_fps > 0.0 { active_fps } else { fallback_fps };
    let video_range =
        if video_range <= u32::from(u8::MAX) { video_range as u8 } else { fallback_video_range };
    let color_space =
        if color_space <= u32::from(u8::MAX) { color_space as u8 } else { fallback_color_space };
    (width, height, fps, video_range, color_space)
}

#[inline]
fn apply_camera_contract_snapshot(_stats: &mut StatsSnapshot) {
    #[cfg(target_os = "ios")]
    {
        let mut snap = OxideCamContractSnapshot::default();
        if unsafe { oxide_cam_get_contract_snapshot(&mut snap) } == 1 {
            let (width, height, fps, video_range, color_space) = merge_camera_contract_fields(
                _stats.cam_width,
                _stats.cam_height,
                _stats.cam_fps,
                _stats.cam_video_range,
                _stats.cam_color_space,
                snap.active_width,
                snap.active_height,
                snap.active_fps,
                snap.video_range,
                snap.color_space,
            );
            _stats.cam_width = width;
            _stats.cam_height = height;
            _stats.cam_fps = fps;
            _stats.cam_video_range = video_range;
            _stats.cam_color_space = color_space;
        }
    }
}

#[inline]
fn stats_snapshot_from_perf(perf_stats: metal::PerfStats, camera_running: bool) -> StatsSnapshot {
    let mut stats = StatsSnapshot {
        draws: perf_stats.draws,
        damage_pct: perf_stats.damage_pct,
        damage_rects: perf_stats.damage_rects,
        cam_coverage_pct: perf_stats.cam_coverage_pct,
        cam_blur_ms: perf_stats.blur_ms as f32,
        cam_blur_updates: perf_stats.blur_updates,
        cam_update_period_ms: perf_stats.blur_period_ms,
        cam_paused: perf_stats.cam_paused,
        cam_low_power: perf_stats.low_power,
        cam_thermal: perf_stats.thermal,
        cam_width: perf_stats.cam_width,
        cam_height: perf_stats.cam_height,
        cam_bit_depth: perf_stats.cam_bit_depth,
        cam_matrix: perf_stats.cam_matrix,
        cam_video_range: perf_stats.cam_video_range,
        cam_color_space: perf_stats.cam_color_space,
        cam_running: if camera_running { 1 } else { 0 },
        cam_fps: if perf_stats.blur_period_ms > 0 {
            1000.0 / (perf_stats.blur_period_ms as f32)
        } else {
            0.0
        },
        ..StatsSnapshot::default()
    };
    apply_camera_stage_perf(&mut stats, &perf_stats);
    stats
}

fn render_camera_benchmark_fast_path(
    renderer: &mut metal::MetalRenderer,
    camera_running: bool,
    w: u32,
    h: u32,
    scale: f32,
    drawable_ptr: *mut ::libc::c_void,
) -> Result<StatsSnapshot, ::libc::c_int> {
    let perf_stats = match with_perf_signpost("camera.renderer.direct_preview", || unsafe {
        renderer.render_camera_preview_direct(drawable_ptr.cast(), w, h, scale)
    }) {
        Ok(stats) => stats,
        Err(_) => return Err(-4),
    };
    Ok(stats_snapshot_from_perf(perf_stats, camera_running))
}

fn camera_preview_plan(
    renderer: &metal::MetalRenderer,
    camera_running: bool,
    w: u32,
    h: u32,
    scale: f32,
) -> ::libc::c_int {
    if renderer.camera_preview_needs_drawable(w, h, scale, camera_running) {
        1
    } else {
        0
    }
}

fn camera_preview_plan_reason(
    renderer: &metal::MetalRenderer,
    camera_running: bool,
    w: u32,
    h: u32,
    scale: f32,
) -> ::libc::c_int {
    renderer.camera_preview_draw_reason(w, h, scale, camera_running) as ::libc::c_int
}

#[cfg(target_os = "ios")]
impl TimeService for IosTime {
    fn monotonic_now(&self) -> Duration {
        let mut info = mach2::mach_time::mach_timebase_info_data_t { numer: 0, denom: 0 };
        let status = unsafe { mach2::mach_time::mach_timebase_info(&mut info) };
        if status != mach2::kern_return::KERN_SUCCESS || info.denom == 0 {
            return Duration::from_nanos(0);
        }
        let time = unsafe { mach2::mach_time::mach_absolute_time() };
        let nanos = time.saturating_mul(u64::from(info.numer)) / u64::from(info.denom);
        Duration::from_nanos(nanos)
    }
}

#[cfg(target_os = "ios")]
struct SensorRuntime {
    bridge: Arc<SensorBridge>,
    telemetry: Weak<TelemetryHub>,
    _binding: oxide_permissions::sensors::SensorPermissionBinding,
    location: IosLocation,
    motion: IosMotion,
    bluetooth: IosBluetooth,
    push: IosPushManager,
    location_running: bool,
    motion_running: bool,
}

#[cfg(target_os = "ios")]
impl SensorRuntime {
    fn new(
        bridge: Arc<SensorBridge>,
        perms: Arc<PermissionManager>,
        telemetry: Weak<TelemetryHub>,
        restore_id: Option<&str>,
    ) -> Self {
        let binding = bridge.bind_permissions(&perms);
        let bluetooth =
            if let Some(id) = restore_id { bluetooth_with_restoration(id) } else { IosBluetooth };
        let mut runtime = Self {
            bluetooth,
            location: IosLocation,
            motion: IosMotion,
            push: IosPushManager,
            telemetry,
            bridge: Arc::clone(&bridge),
            _binding: binding,
            location_running: false,
            motion_running: false,
        };
        runtime.install_streams();
        runtime.install_push_token_bridge();
        runtime.refresh_all();
        runtime
    }

    fn install_streams(&mut self) {
        let telemetry_loc = self.telemetry.clone();
        let loc_bridge = Arc::clone(&self.bridge);
        self.location.subscribe(Box::new(move |event| {
            loc_bridge.handle_location_event(event);
            if let Some(tele) = telemetry_loc.upgrade() {
                tele.update_sensors(Some(loc_bridge.snapshot()));
            }
        }));

        let telemetry_motion = self.telemetry.clone();
        let motion_bridge = Arc::clone(&self.bridge);
        self.motion.subscribe(Box::new(move |sample| {
            motion_bridge.handle_motion_sample(sample);
            if let Some(tele) = telemetry_motion.upgrade() {
                tele.update_sensors(Some(motion_bridge.snapshot()));
            }
        }));

        let telemetry_bt = self.telemetry.clone();
        let bt_bridge = Arc::clone(&self.bridge);
        self.bluetooth.subscribe_events(Box::new(move |event| {
            bt_bridge.handle_bluetooth_event(event);
            if let Some(tele) = telemetry_bt.upgrade() {
                tele.update_sensors(Some(bt_bridge.snapshot()));
            }
        }));
        let powered = self.bluetooth.powered_on();
        self.bridge.handle_bluetooth_event(BluetoothEvent::StateChanged { powered_on: powered });

        let telemetry_push = self.telemetry.clone();
        let push_bridge = Arc::clone(&self.bridge);
        self.push.subscribe(Box::new(move |notification| {
            push_bridge.handle_push_notification(notification);
            if let Some(tele) = telemetry_push.upgrade() {
                tele.update_sensors(Some(push_bridge.snapshot()));
            }
        }));
    }

    fn install_push_token_bridge(&self) {
        let bridge_cell = SENSOR_PUSH_BRIDGE.get_or_init(|| std::sync::Mutex::new(Weak::new()));
        if let Ok(mut slot) = bridge_cell.lock() {
            *slot = Arc::downgrade(&self.bridge);
        }
        let telemetry_cell =
            SENSOR_TELEMETRY_BRIDGE.get_or_init(|| std::sync::Mutex::new(Weak::new()));
        if let Ok(mut slot) = telemetry_cell.lock() {
            *slot = self.telemetry.clone();
        }
        oxide_host_set_push_token_callback(Some(sensor_push_token_cb));
    }

    fn permission_allowed(&self, domain: PermissionDomain) -> bool {
        matches!(
            self.bridge.permission_status(domain),
            Some(PermissionStatus::Authorized | PermissionStatus::Limited)
        )
    }

    fn refresh_location(&mut self) {
        let allowed = self.permission_allowed(PermissionDomain::Location);
        if allowed && !self.location_running {
            let opts = LocationOptions::default();
            if let Err(err) = self.location.start(opts) {
                ios_log(&format!("location start failed: {err:?}"));
            } else {
                self.location_running = true;
            }
        } else if !allowed && self.location_running {
            self.location.stop();
            self.location_running = false;
        }
    }

    fn refresh_motion(&mut self) {
        let allowed = self.permission_allowed(PermissionDomain::Motion);
        if allowed && !self.motion_running {
            if let Err(err) = self.motion.start() {
                ios_log(&format!("motion start failed: {err:?}"));
            } else {
                self.motion_running = true;
            }
        } else if !allowed && self.motion_running {
            self.motion.stop();
            self.motion_running = false;
        }
    }

    fn refresh_push(&self) {
        if self.permission_allowed(PermissionDomain::Notifications) {
            self.push.register();
            self.install_push_token_bridge();
            if let Some(token) = self.push.device_token() {
                self.bridge.set_push_token(Some(token));
            }
        } else {
            self.bridge.set_push_token(None);
        }
    }

    fn refresh_all(&mut self) {
        self.refresh_location();
        self.refresh_motion();
        self.refresh_push();
        if let Some(tele) = self.telemetry.upgrade() {
            tele.update_sensors(Some(self.bridge.snapshot()));
        }
    }

    fn suspend(&mut self) {
        if self.location_running {
            self.location.stop();
            self.location_running = false;
        }
        if self.motion_running {
            self.motion.stop();
            self.motion_running = false;
        }
        self.bluetooth.stop_scan();
        self.bridge.prune_bluetooth();
        if let Some(tele) = self.telemetry.upgrade() {
            tele.update_sensors(Some(self.bridge.snapshot()));
        }
    }

    fn resume(&mut self) {
        self.refresh_all();
    }

    fn trim_memory(&self) {
        self.bridge.trim_memory();
        if let Some(tele) = self.telemetry.upgrade() {
            tele.update_sensors(Some(self.bridge.snapshot()));
        }
    }

    fn handle_permission_change(&mut self, state: PermissionState) {
        match state.domain {
            PermissionDomain::Location => self.refresh_location(),
            PermissionDomain::Motion => self.refresh_motion(),
            PermissionDomain::Notifications => self.refresh_push(),
            PermissionDomain::Bluetooth => {
                let powered = self.bluetooth.powered_on();
                self.bridge
                    .handle_bluetooth_event(BluetoothEvent::StateChanged { powered_on: powered });
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "ios")]
struct NetworkRuntime {
    reachability: IosReachability,
    session: QuicSessionManager,
    telemetry: Weak<TelemetryHub>,
    _reachability_sub: ReachabilitySubscription,
    paused: bool,
}

#[cfg(target_os = "ios")]
impl NetworkRuntime {
    fn new(
        manager: Arc<ReachabilityManager>,
        telemetry: Arc<TelemetryHub>,
    ) -> Result<Self, PlatformError> {
        let reachability = IosReachability::new(manager);
        reachability.start()?;
        telemetry.update_reachability(reachability.manager().snapshot());
        let telemetry_weak = Arc::downgrade(&telemetry);
        let reachability_sub = reachability.manager().subscribe({
            let tel = telemetry_weak.clone();
            move |snapshot| {
                if let Some(hub) = tel.upgrade() {
                    hub.update_reachability(snapshot);
                }
            }
        });
        let session = QuicSessionManager::with_default_clock();
        telemetry.update_network_metrics(Some(session.metrics()));
        Ok(Self {
            reachability,
            session,
            telemetry: telemetry_weak,
            _reachability_sub: reachability_sub,
            paused: false,
        })
    }

    fn tick(&mut self, now_ms: u64) {
        if self.paused {
            return;
        }
        self.session.tick(now_ms);
        for packet in self.session.drain_outbound() {
            match packet.kind {
                PacketKind::HandshakeInit | PacketKind::HandshakeRetry => {
                    let response = HandshakeResponse { accepted: true, session_id: Some(1) };
                    self.session.on_handshake_response(response, now_ms);
                }
                PacketKind::TimeSyncProbe => {
                    let sample = TimeSyncSample {
                        client_send_ms: packet.timestamp_ms,
                        server_recv_ms: packet.timestamp_ms + 4,
                        server_send_ms: packet.timestamp_ms + 6,
                        client_recv_ms: now_ms.max(packet.timestamp_ms + 8),
                    };
                    self.session.on_time_sync_response(sample);
                }
            }
        }
        let metrics = self.session.metrics();
        if let Some(tele) = self.telemetry.upgrade() {
            tele.update_network_metrics(Some(metrics));
        }
    }

    fn metrics(&self) -> oxide_networking::QuicSessionMetrics {
        self.session.metrics()
    }

    fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.reachability.stop();
        self.paused = true;
    }

    fn resume(&mut self, reason: TelemetryCommandReason) {
        if self.paused {
            match self.reachability.start() {
                Ok(_) => self.paused = false,
                Err(err) => ios_log(&format!("reachability restart failed: {err}")),
            }
        }
        if matches!(
            reason,
            TelemetryCommandReason::HealthDegraded
                | TelemetryCommandReason::ScheduledRecovery
                | TelemetryCommandReason::MemoryPressure(MemoryPressureLevel::Critical)
        ) {
            self.session = QuicSessionManager::with_default_clock();
        }
        if let Some(tele) = self.telemetry.upgrade() {
            tele.update_reachability(self.reachability.manager().snapshot());
            tele.update_network_metrics(Some(self.session.metrics()));
        }
    }

    fn stop(&mut self) {
        self.reachability.stop();
        self.paused = true;
    }
}

#[cfg(target_os = "ios")]
impl Drop for NetworkRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

#[no_mangle]
pub extern "C" fn rust_entry(
    _argc: ::libc::c_int,
    _argv: *mut *mut ::core::ffi::c_char,
) -> ::libc::c_int {
    #[cfg(target_os = "ios")]
    unsafe {
        #[cfg(feature = "tokio-runtime")]
        {
            oxide_platform_ios::init_tokio_spawn();
        }
        ios_log("rust_entry: calling oxide_host_start");
        let rc = oxide_host_start(_argc, _argv) as ::libc::c_int;
        ios_log(&format!("rust_entry: oxide_host_start returned {}", rc));
        return rc;
    }
    #[cfg(not(target_os = "ios"))]
    {
        -1 as ::libc::c_int
    }
}

// ---- Window resize callback bridge ----

type WindowResizedCb =
    extern "C" fn(w: f32, h: f32, scale: f32, safe_l: f32, safe_t: f32, safe_r: f32, safe_b: f32);

static WINDOW_CB: std::sync::OnceLock<std::sync::Mutex<Option<WindowResizedCb>>> =
    std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn oxide_host_set_window_resized_callback(cb: Option<WindowResizedCb>) {
    let slot = WINDOW_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("window cb mutex poisoned") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_window_resized(
    w: f32,
    h: f32,
    scale: f32,
    safe_l: f32,
    safe_t: f32,
    safe_r: f32,
    safe_b: f32,
) {
    if let Some(cb) = WINDOW_CB.get().and_then(|m| *m.lock().expect("window cb mutex poisoned")) {
        cb(w, h, scale, safe_l, safe_t, safe_r, safe_b);
    } else {
        eprintln!(
            "[Oxide] window resized: {:.1}x{:.1} scale={:.2} safe=({:.1},{:.1},{:.1},{:.1})",
            w, h, scale, safe_l, safe_t, safe_r, safe_b
        );
    }
}

// ---- Text/IME callback bridges ----

type TextCommitCb = extern "C" fn(text_ptr: *const u8, text_len: usize);
type TextCompositionCb = extern "C" fn(start: u32, end: u32, text_ptr: *const u8, text_len: usize);
type TextSelectionCb = extern "C" fn(start: u32, end: u32);
type IMEShownCb = extern "C" fn(x: f32, y: f32, w: f32, h: f32);
type IMEHiddenCb = extern "C" fn();

static TEXT_COMMIT_CB: std::sync::OnceLock<std::sync::Mutex<Option<TextCommitCb>>> =
    std::sync::OnceLock::new();
static TEXT_COMPOSE_CB: std::sync::OnceLock<std::sync::Mutex<Option<TextCompositionCb>>> =
    std::sync::OnceLock::new();
static TEXT_SELECT_CB: std::sync::OnceLock<std::sync::Mutex<Option<TextSelectionCb>>> =
    std::sync::OnceLock::new();
static IME_SHOWN_CB: std::sync::OnceLock<std::sync::Mutex<Option<IMEShownCb>>> =
    std::sync::OnceLock::new();
static IME_HIDDEN_CB: std::sync::OnceLock<std::sync::Mutex<Option<IMEHiddenCb>>> =
    std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn oxide_host_set_text_commit_callback(cb: Option<TextCommitCb>) {
    let slot = TEXT_COMMIT_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("commit cb mutex") = cb;
}

// ---- Permissions callback bridge ----
type PermCb = extern "C" fn(domain: u32, status: u32);
static PERM_CB: std::sync::OnceLock<std::sync::Mutex<Option<PermCb>>> = std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn oxide_host_set_perm_callback(cb: Option<PermCb>) {
    let slot = PERM_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("perm cb mutex") = cb;
}

// ---- Push notifications callback bridge ----
type PushTokenCb = extern "C" fn(provider: u32, token_ptr: *const u8, token_len: usize);
type PushNotifyCb = extern "C" fn(json_ptr: *const u8, json_len: usize);
static PUSH_TOKEN_CB: std::sync::OnceLock<std::sync::Mutex<Option<PushTokenCb>>> =
    std::sync::OnceLock::new();
static PUSH_NOTIFY_CB: std::sync::OnceLock<std::sync::Mutex<Option<PushNotifyCb>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "ios")]
static SENSOR_PUSH_BRIDGE: std::sync::OnceLock<std::sync::Mutex<Weak<SensorBridge>>> =
    std::sync::OnceLock::new();
#[cfg(target_os = "ios")]
static SENSOR_TELEMETRY_BRIDGE: std::sync::OnceLock<std::sync::Mutex<Weak<TelemetryHub>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "ios")]
fn push_provider_from_u32(value: u32) -> PushProvider {
    match value {
        1 => PushProvider::Fcm,
        _ => PushProvider::Apns,
    }
}

#[cfg(target_os = "ios")]
extern "C" fn sensor_push_token_cb(provider: u32, ptr: *const u8, len: usize) {
    unsafe {
        oxide_platform_ios::oxide_push_token_trampoline(provider, ptr, len);
    }
    if let Some(cell) = SENSOR_PUSH_BRIDGE.get() {
        if let Ok(weak) = cell.lock() {
            if let Some(bridge) = weak.upgrade() {
                if ptr.is_null() || len == 0 {
                    bridge.set_push_token(None);
                    if let Some(tcell) = SENSOR_TELEMETRY_BRIDGE.get() {
                        if let Ok(tweak) = tcell.lock() {
                            if let Some(tele) = tweak.upgrade() {
                                tele.update_sensors(Some(bridge.snapshot()));
                            }
                        }
                    }
                    return;
                }
                let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
                let value = String::from_utf8_lossy(bytes).into_owned();
                let provider = push_provider_from_u32(provider);
                bridge.set_push_token(Some(PushToken { provider, value }));
                if let Some(tcell) = SENSOR_TELEMETRY_BRIDGE.get() {
                    if let Ok(tweak) = tcell.lock() {
                        if let Some(tele) = tweak.upgrade() {
                            tele.update_sensors(Some(bridge.snapshot()));
                        }
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_push_token_callback(cb: Option<PushTokenCb>) {
    let slot = PUSH_TOKEN_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("push token cb mutex") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_push_notify_callback(cb: Option<PushNotifyCb>) {
    let slot = PUSH_NOTIFY_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("push notify cb mutex") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_push_token(provider: u32, ptr: *const u8, len: usize) {
    if let Some(cb) = PUSH_TOKEN_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(provider, ptr, len);
    } else if let Ok(s) = std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }) {
        eprintln!("[Oxide] push token (prov={}): {}", provider, s);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_push_notify(ptr: *const u8, len: usize) {
    if let Some(cb) = PUSH_NOTIFY_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(ptr, len);
    } else if let Ok(s) = std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }) {
        eprintln!("[Oxide] push notify: {}", s);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_perm(domain: u32, status: u32) {
    if let Some(cb) = PERM_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(domain, status);
    } else {
        eprintln!("[Oxide] perm domain={} status={}", domain, status);
    }
}

// ---- Input callbacks (touch/pointer/key) ----

type TouchCb = extern "C" fn(
    id: u64,
    phase: u32, // 0 Start, 1 Move, 2 End, 3 Cancel
    x: f32,
    y: f32,
    pressure: f32,
    has_pressure: u8,
    tilt_alt: f32,
    tilt_azi: f32,
    has_tilt: u8,
    device: u32, // 0 Finger, 1 Pencil, 2 Mouse
    timestamp_ns: u64,
);

type PointerCb = extern "C" fn(
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    buttons: u32,
    modifiers: u32,
    timestamp_ns: u64,
);

type KeyCb = extern "C" fn(
    code: u32,
    chars_ptr: *const u8,
    chars_len: usize,
    repeat: u8,
    modifiers: u32,
    timestamp_ns: u64,
);

static TOUCH_CB: std::sync::OnceLock<std::sync::Mutex<Option<TouchCb>>> =
    std::sync::OnceLock::new();
static POINTER_CB: std::sync::OnceLock<std::sync::Mutex<Option<PointerCb>>> =
    std::sync::OnceLock::new();
static KEY_CB: std::sync::OnceLock<std::sync::Mutex<Option<KeyCb>>> = std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn oxide_host_set_touch_callback(cb: Option<TouchCb>) {
    let slot = TOUCH_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("touch cb mutex") = cb;
}

// ---- Bluetooth (CoreBluetooth) callbacks ----
type BleStateCb = extern "C" fn(powered_on: u8);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OxideBleScanInfo {
    pub id: [u8; 16],
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub rssi_dbm: i16,
    pub services_ptr: *const u8,
    pub service_count: usize,
    pub manufacturer_ptr: *const u8,
    pub manufacturer_len: usize,
    pub connectable: u8,
}

type BleDiscoveredCb = extern "C" fn(info: *const OxideBleScanInfo);
type BleRestoredCb = extern "C" fn(infos: *const OxideBleScanInfo, count: usize);
type BleConnCb = extern "C" fn(id_ptr: *const u8);
type BleDiscCb = extern "C" fn(id_ptr: *const u8);
type BleNotifyCb = extern "C" fn(
    id_ptr: *const u8,
    svc_ptr: *const u8,
    chr_ptr: *const u8,
    data_ptr: *const u8,
    data_len: usize,
);

static BLE_STATE_CB: std::sync::OnceLock<std::sync::Mutex<Option<BleStateCb>>> =
    std::sync::OnceLock::new();
static BLE_DISCOVERED_CB: std::sync::OnceLock<std::sync::Mutex<Option<BleDiscoveredCb>>> =
    std::sync::OnceLock::new();
static BLE_RESTORED_CB: std::sync::OnceLock<std::sync::Mutex<Option<BleRestoredCb>>> =
    std::sync::OnceLock::new();
static BLE_CONN_CB: std::sync::OnceLock<std::sync::Mutex<Option<BleConnCb>>> =
    std::sync::OnceLock::new();
static BLE_DISC_CB: std::sync::OnceLock<std::sync::Mutex<Option<BleDiscCb>>> =
    std::sync::OnceLock::new();
static BLE_NOTIFY_CB: std::sync::OnceLock<std::sync::Mutex<Option<BleNotifyCb>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "ios")]
const PERMISSION_DOMAINS: [PermissionDomain; 8] = [
    PermissionDomain::Camera,
    PermissionDomain::Microphone,
    PermissionDomain::Location,
    PermissionDomain::Bluetooth,
    PermissionDomain::Motion,
    PermissionDomain::Notifications,
    PermissionDomain::Contacts,
    PermissionDomain::MediaLibrary,
];

#[cfg(target_os = "ios")]
fn initialize_permissions(manager: &PermissionManager) -> Vec<PermissionState> {
    for domain in PERMISSION_DOMAINS {
        manager.status(domain);
    }
    manager.snapshot()
}

#[cfg(target_os = "ios")]
fn install_permission_subscriptions(manager: Arc<PermissionManager>) {
    let mut handles = Vec::new();
    for domain in PERMISSION_DOMAINS {
        let handle = manager.subscribe(domain, move |state| {
            let _ = with_app_mut(|app| {
                app.permission_states.retain(|s| s.domain != state.domain);
                app.permission_states.push(state);
                if let Some(router) = app.router.as_mut() {
                    router.permissions_update(&app.permission_states);
                }
                if let Some(runtime) = app.sensor_runtime.as_mut() {
                    runtime.handle_permission_change(state);
                }
                if let Some(telemetry) = app.telemetry.as_ref() {
                    telemetry.update_permissions(app.permission_states.clone());
                }
            });
        });
        handles.push(handle);
    }
    let _ = with_app_mut(|app| {
        app.permission_subs = handles;
    });
}

#[no_mangle]
pub extern "C" fn oxide_host_ble_set_state_cb(cb: Option<BleStateCb>) {
    let s = BLE_STATE_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().unwrap() = cb;
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_set_discovered_cb(cb: Option<BleDiscoveredCb>) {
    let s = BLE_DISCOVERED_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().unwrap() = cb;
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_set_restored_cb(cb: Option<BleRestoredCb>) {
    let s = BLE_RESTORED_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().unwrap() = cb;
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_set_connected_cb(cb: Option<BleConnCb>) {
    let s = BLE_CONN_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().unwrap() = cb;
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_set_disconnected_cb(cb: Option<BleDiscCb>) {
    let s = BLE_DISC_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().unwrap() = cb;
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_set_notify_cb(cb: Option<BleNotifyCb>) {
    let s = BLE_NOTIFY_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().unwrap() = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_state(powered_on: u8) {
    if let Some(cb) = BLE_STATE_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(powered_on);
    }
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_discovered(info: *const OxideBleScanInfo) {
    if let Some(cb) = BLE_DISCOVERED_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(info);
    }
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_restored(infos: *const OxideBleScanInfo, count: usize) {
    if let Some(cb) = BLE_RESTORED_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(infos, count);
    }
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_connected(id: *const u8) {
    if let Some(cb) = BLE_CONN_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(id);
    }
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_disconnected(id: *const u8) {
    if let Some(cb) = BLE_DISC_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(id);
    }
}
#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_notified(
    id: *const u8,
    svc: *const u8,
    chr: *const u8,
    data: *const u8,
    len: usize,
) {
    if let Some(cb) = BLE_NOTIFY_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(id, svc, chr, data, len);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_pointer_callback(cb: Option<PointerCb>) {
    let slot = POINTER_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("pointer cb mutex") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_key_callback(cb: Option<KeyCb>) {
    let slot = KEY_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("key cb mutex") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_touch(
    id: u64,
    phase: u32,
    x: f32,
    y: f32,
    pressure: f32,
    has_pressure: u8,
    tilt_alt: f32,
    tilt_azi: f32,
    has_tilt: u8,
    device: u32,
    timestamp_ns: u64,
) {
    if let Some(cb) = TOUCH_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(
            id,
            phase,
            x,
            y,
            pressure,
            has_pressure,
            tilt_alt,
            tilt_azi,
            has_tilt,
            device,
            timestamp_ns,
        );
    } else {
        eprintln!("[Oxide] touch id={} phase={} x={} y={}", id, phase, x, y);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_pointer(
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    buttons: u32,
    modifiers: u32,
    timestamp_ns: u64,
) {
    if let Some(cb) = POINTER_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(x, y, dx, dy, buttons, modifiers, timestamp_ns);
    } else {
        eprintln!("[Oxide] pointer x={} y={} dx={} dy={}", x, y, dx, dy);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_key(
    code: u32,
    chars_ptr: *const u8,
    chars_len: usize,
    repeat: u8,
    modifiers: u32,
    timestamp_ns: u64,
) {
    if let Some(cb) = KEY_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(code, chars_ptr, chars_len, repeat, modifiers, timestamp_ns);
    } else {
        let s = if chars_len > 0 {
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(chars_ptr, chars_len) })
                .unwrap_or("")
        } else {
            ""
        };
        eprintln!("[Oxide] key code={} chars='{}' repeat={} mods={}", code, s, repeat, modifiers);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_text_composition_callback(cb: Option<TextCompositionCb>) {
    let slot = TEXT_COMPOSE_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("compose cb mutex") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_text_selection_callback(cb: Option<TextSelectionCb>) {
    let slot = TEXT_SELECT_CB.get_or_init(|| std::sync::Mutex::new(None));
    *slot.lock().expect("select cb mutex") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_ime_callbacks(
    shown: Option<IMEShownCb>,
    hidden: Option<IMEHiddenCb>,
) {
    let s = IME_SHOWN_CB.get_or_init(|| std::sync::Mutex::new(None));
    *s.lock().expect("ime shown mutex") = shown;
    let h = IME_HIDDEN_CB.get_or_init(|| std::sync::Mutex::new(None));
    *h.lock().expect("ime hidden mutex") = hidden;
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_text_commit(ptr: *const u8, len: usize) {
    if let Some(cb) = TEXT_COMMIT_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(ptr, len);
    } else {
        if let Ok(s) = std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }) {
            eprintln!("[Oxide] text commit: {}", s);
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_text_composition(
    start: u32,
    end: u32,
    ptr: *const u8,
    len: usize,
) {
    if let Some(cb) = TEXT_COMPOSE_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(start, end, ptr, len);
    } else {
        if let Ok(s) = std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }) {
            eprintln!("[Oxide] text composition: [{}..{}] {}", start, end, s);
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_text_selection(start: u32, end: u32) {
    if let Some(cb) = TEXT_SELECT_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(start, end);
    } else {
        eprintln!("[Oxide] selection: [{}..{}]", start, end);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_ime_shown(x: f32, y: f32, w: f32, h: f32) {
    if let Some(cb) = IME_SHOWN_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb(x, y, w, h);
    } else {
        eprintln!("[Oxide] IME shown at {},{} size {}x{}", x, y, w, h);
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_ime_hidden() {
    if let Some(cb) = IME_HIDDEN_CB.get().and_then(|m| *m.lock().unwrap()) {
        cb();
    } else {
        eprintln!("[Oxide] IME hidden");
    }
}

// ===== Renderer + scene router integration =====

extern "C" {
    fn oxide_host_resource_read(
        name: *const ::libc::c_char,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> ::libc::c_int;
    fn oxide_host_string_free(p: *mut u8);
}

struct MtlUploader {
    renderer: *mut metal::MetalRenderer,
}

unsafe impl Send for MtlUploader {}
unsafe impl Sync for MtlUploader {}

impl ui::elements::ImageUploader for MtlUploader {
    fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> gfx_api::ImageHandle {
        unsafe { (*self.renderer).image_create_a8(w, h, data, row_bytes) }
    }

    fn update_a8(
        &mut self,
        handle: gfx_api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        unsafe { (*self.renderer).image_update_a8(handle, x, y, w, h, data, row_bytes) }
    }
}

#[derive(Clone, Copy, Default)]
struct StatsSnapshot {
    fps: f32,
    draws: u32,
    anims: u32,
    damage_pct: f32,
    damage_rects: u32,
    cam_coverage_pct: f32,
    cam_blur_ms: f32,
    cam_blur_updates: u32,
    cam_update_period_ms: u32,
    cam_paused: u8,
    cam_low_power: u8,
    cam_thermal: u8,
    cam_width: u32,
    cam_height: u32,
    cam_bit_depth: u8,
    cam_matrix: u8,
    cam_video_range: u8,
    cam_color_space: u8,
    cam_running: u8,
    cam_fps: f32,
    cam_poll_submissions_ms: f32,
    cam_fetch_ms: f32,
    cam_setup_ms: f32,
    cam_encode_quad_ms: f32,
    cam_command_buffer_ms: f32,
    cam_encoder_ms: f32,
    cam_encode_bind_ms: f32,
    cam_encode_draw_ms: f32,
    cam_end_encoding_ms: f32,
    cam_present_ms: f32,
    cam_commit_ms: f32,
    cam_gpu_ms: f32,
    cam_capture_total_ms: f32,
    cam_capture_sample_setup_ms: f32,
    cam_capture_lock_ms: f32,
    cam_capture_texture_bridge_ms: f32,
    cam_capture_publish_ms: f32,
    cam_capture_publish_lock_ms: f32,
    cam_capture_publish_texture_refs_ms: f32,
    cam_capture_publish_pixel_buffer_ms: f32,
    cam_capture_frame_delivery_ms: f32,
    cam_sample_delivery_pool_bytes: u64,
    cam_sample_delivery_pool_surfaces: u32,
    cam_active_sample_surface_bytes: u64,
    cam_active_sample_surface_surfaces: u32,
    cam_active_sample_buffers: u32,
    cam_peak_active_sample_surface_bytes: u64,
    cam_peak_active_sample_surface_surfaces: u32,
    cam_peak_active_sample_buffers: u32,
    cam_sample_delivery_total_samples: u32,
    cam_sample_delivery_reused_frames: u32,
    cam_sample_delivery_reused_surfaces: u32,
    cam_sample_delivery_max_reuse_gap_frames: u32,
    cam_retained_sample_surface_bytes: u64,
    cam_retained_sample_surface_surfaces: u32,
    cam_retained_published_slot_surface_bytes: u64,
    cam_retained_published_slot_surfaces: u32,
    cam_retained_latest_pixel_buffer_surface_bytes: u64,
    cam_retained_latest_pixel_buffer_surface_surfaces: u32,
    cam_latest_published_generation: u64,
    cam_latest_published_timestamp_ns: u64,
    renderer_memory_total_bytes: u64,
    renderer_memory_draw_targets_bytes: u64,
    renderer_memory_draw_target_main_bytes: u64,
    renderer_memory_draw_target_msaa_bytes: u64,
    renderer_memory_effect_targets_bytes: u64,
    renderer_memory_effect_prepass_bytes: u64,
    renderer_memory_effect_blur_chain_bytes: u64,
    renderer_memory_live_camera_bytes: u64,
    renderer_memory_camera_cache_bytes: u64,
    renderer_memory_camera_blur_cache_bytes: u64,
    renderer_memory_camera_transition_cache_bytes: u64,
    renderer_memory_benchmark_camera_bytes: u64,
    renderer_memory_layer_cache_bytes: u64,
    renderer_memory_image_cache_bytes: u64,
    renderer_memory_buffer_bytes: u64,
    renderer_pending_command_buffers: u32,
    renderer_pending_present_drawables: u32,
    renderer_pending_present_textures: u32,
    renderer_preview_submission_depth: u32,
    renderer_preview_submission_skipped: u32,
    renderer_preview_submission_frame_age_ms: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Default)]
struct WindowMetrics {
    width_dp: f32,
    height_dp: f32,
    scale: f32,
    safe_left: f32,
    safe_top: f32,
    safe_right: f32,
    safe_bottom: f32,
}

#[derive(Clone, Copy, Default)]
struct PointerSample {
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    buttons: u32,
}

#[derive(Default)]
struct TouchResult {
    pointer: Option<PointerSample>,
    double_tap: bool,
}

#[derive(Clone, Copy)]
struct TouchTrack {
    id: u64,
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    start_ms: u64,
    last_ms: u64,
}

impl TouchTrack {
    fn new(id: u64, x: f32, y: f32, ts_ns: u64) -> Self {
        let ms = ts_ns / 1_000_000;
        Self { id, start_x: x, start_y: y, last_x: x, last_y: y, start_ms: ms, last_ms: ms }
    }
}

#[derive(Clone, Copy)]
struct TapRecord {
    ts_ms: u64,
    x: f32,
    y: f32,
}

#[derive(Default)]
struct PrimaryTouchTracker {
    active: Option<TouchTrack>,
    last_tap: Option<TapRecord>,
}

impl PrimaryTouchTracker {
    fn on_event(&mut self, id: u64, phase: u32, x: f32, y: f32, ts_ns: u64) -> TouchResult {
        let mut result = TouchResult::default();
        let ms = ts_ns / 1_000_000;
        match phase {
            0 => {
                if self.active.is_none() {
                    self.active = Some(TouchTrack::new(id, x, y, ts_ns));
                    result.pointer = Some(PointerSample { x, y, dx: 0.0, dy: 0.0, buttons: 1 });
                }
            }
            1 => {
                if let Some(mut track) = self.active {
                    if track.id == id {
                        let dx = x - track.last_x;
                        let dy = y - track.last_y;
                        track.last_x = x;
                        track.last_y = y;
                        track.last_ms = ms;
                        result.pointer = Some(PointerSample { x, y, dx, dy, buttons: 1 });
                        self.active = Some(track);
                    }
                }
            }
            2 | 3 => {
                if let Some(track) = self.active {
                    if track.id == id {
                        let dx = x - track.last_x;
                        let dy = y - track.last_y;
                        result.pointer = Some(PointerSample { x, y, dx, dy, buttons: 0 });
                        let total_dx = x - track.start_x;
                        let total_dy = y - track.start_y;
                        let moved_sq = total_dx * total_dx + total_dy * total_dy;
                        let dur_ms = ms.saturating_sub(track.start_ms);
                        if dur_ms <= 300 && moved_sq <= 36.0 {
                            let tapped = TapRecord { ts_ms: ms, x, y };
                            if let Some(prev) = self.last_tap {
                                let dt = tapped.ts_ms.saturating_sub(prev.ts_ms);
                                let dx = tapped.x - prev.x;
                                let dy = tapped.y - prev.y;
                                if dt <= 360 && (dx * dx + dy * dy) <= 144.0 {
                                    result.double_tap = true;
                                }
                            }
                            self.last_tap = Some(tapped);
                        }
                        self.active = None;
                    }
                }
                if phase == 3 {
                    self.active = None;
                }
            }
            _ => {}
        }
        result
    }
}

struct AppState {
    renderer: Option<Box<metal::MetalRenderer>>,
    router: Option<test_scenes::Router<MtlUploader>>,
    benchmark_scene_index: u32,
    last_ms: u64,
    inited: bool,
    benchmark_mode: bool,
    last_stats: StatsSnapshot,
    window: WindowMetrics,
    space_down: bool,
    reduce_motion_on: bool,
    reduce_motion_dirty: bool,
    touch: PrimaryTouchTracker,
    memory_warnings: u32,
    overlay_visible: bool,
    overlay_dirty: bool,
    snapshot_status: String,
    camera_running: bool,
    camera_render_mode: metal::CameraRenderMode,
    camera_texture_source: metal::CameraTextureSource,
    permissions: Option<Arc<PermissionManager>>,
    permission_subs: Vec<PermissionSubscription>,
    permission_states: Vec<PermissionState>,
    sensors: Option<Arc<SensorBridge>>,
    networking: Option<Arc<ReachabilityManager>>,
    network_metrics: Option<oxide_networking::QuicSessionMetrics>,
    telemetry: Option<Arc<TelemetryHub>>,
    telemetry_ops: Option<Arc<TelemetryOperations>>,
    #[cfg(target_os = "ios")]
    sensor_runtime: Option<SensorRuntime>,
    #[cfg(target_os = "ios")]
    network_runtime: Option<NetworkRuntime>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            renderer: None,
            router: None,
            benchmark_scene_index: benchmark_camera_scene_index(),
            last_ms: 0,
            inited: false,
            benchmark_mode: false,
            last_stats: StatsSnapshot::default(),
            window: WindowMetrics::default(),
            space_down: false,
            reduce_motion_on: false,
            reduce_motion_dirty: false,
            touch: PrimaryTouchTracker::default(),
            memory_warnings: 0,
            overlay_visible: true,
            overlay_dirty: false,
            snapshot_status: String::new(),
            camera_running: false,
            camera_render_mode: metal::CameraRenderMode::Nv12Optimized,
            camera_texture_source: metal::CameraTextureSource::Live,
            permissions: None,
            permission_subs: Vec::new(),
            permission_states: Vec::new(),
            sensors: None,
            networking: None,
            network_metrics: None,
            telemetry: None,
            telemetry_ops: None,
            #[cfg(target_os = "ios")]
            sensor_runtime: None,
            #[cfg(target_os = "ios")]
            network_runtime: None,
        }
    }
}

static APP_STATE: std::sync::OnceLock<std::sync::Mutex<AppState>> = std::sync::OnceLock::new();
static PERF_REPORT_JSON: std::sync::OnceLock<std::sync::Mutex<Option<Vec<u8>>>> =
    std::sync::OnceLock::new();

fn app_state() -> &'static std::sync::Mutex<AppState> {
    APP_STATE.get_or_init(|| std::sync::Mutex::new(AppState::default()))
}

fn perf_report_json() -> &'static std::sync::Mutex<Option<Vec<u8>>> {
    PERF_REPORT_JSON.get_or_init(|| std::sync::Mutex::new(None))
}

fn with_app_mut<R>(f: impl FnOnce(&mut AppState) -> R) -> Option<R> {
    app_state().lock().ok().map(|mut guard| f(&mut guard))
}

fn process_telemetry_commands_locked(app: &mut AppState) {
    let Some(ops) = app.telemetry_ops.as_ref() else {
        return;
    };
    let commands = ops.drain_commands();
    if commands.is_empty() {
        return;
    }
    for command in commands {
        match command.action {
            TelemetryAction::PauseSensors => {
                #[cfg(target_os = "ios")]
                {
                    if let Some(runtime) = app.sensor_runtime.as_mut() {
                        runtime.suspend();
                    }
                }
            }
            TelemetryAction::ResumeSensors => {
                #[cfg(target_os = "ios")]
                {
                    if let Some(runtime) = app.sensor_runtime.as_mut() {
                        runtime.resume();
                    }
                }
            }
            TelemetryAction::PauseNetworking => {
                #[cfg(target_os = "ios")]
                {
                    if let Some(runtime) = app.network_runtime.as_mut() {
                        runtime.pause();
                    }
                }
            }
            TelemetryAction::ResumeNetworking => {
                #[cfg(target_os = "ios")]
                {
                    if let Some(runtime) = app.network_runtime.as_mut() {
                        runtime.resume(command.reason);
                        app.network_metrics = Some(runtime.metrics());
                    }
                }
            }
            TelemetryAction::RefreshPermissions => {
                if let Some(manager) = app.permissions.as_ref() {
                    let snapshot = manager.snapshot();
                    app.permission_states = snapshot.clone();
                    if let Some(router) = app.router.as_mut() {
                        router.permissions_update(&app.permission_states);
                    }
                    if let Some(telemetry) = app.telemetry.as_ref() {
                        telemetry.update_permissions(snapshot);
                    }
                    #[cfg(target_os = "ios")]
                    {
                        if let Some(runtime) = app.sensor_runtime.as_mut() {
                            runtime.refresh_all();
                        }
                    }
                }
            }
            TelemetryAction::TrimCaches => {
                if let Some(router) = app.router.as_mut() {
                    router.trim_memory();
                }
                #[cfg(target_os = "ios")]
                {
                    if let Some(runtime) = app.sensor_runtime.as_mut() {
                        runtime.trim_memory();
                    }
                }
            }
            TelemetryAction::FlushMetrics => log_telemetry_metrics(app, command.reason),
        }
    }
}

fn log_telemetry_metrics(app: &AppState, reason: TelemetryCommandReason) {
    if let Some(telemetry) = app.telemetry.as_ref() {
        let snapshot = telemetry.snapshot();
        let network_phase = snapshot
            .network
            .as_ref()
            .map(|metrics| format!("{:?}", metrics.phase))
            .unwrap_or_else(|| "none".to_owned());
        let sensors_ready =
            snapshot.sensors.as_ref().map(|s| s.location.last.is_some()).unwrap_or(false);
        let message = format!(
            "[Telemetry] reason={:?} lifecycle={:?} health={:?} memory={:?} perms={} reachability={:?} sensors_ready={} network_phase={}",
            reason,
            snapshot.operations.lifecycle,
            snapshot.health,
            snapshot.memory_pressure,
            snapshot.permissions.len(),
            snapshot.reachability.state,
            sensors_ready,
            network_phase
        );
        ios_log(&message);
    }
}

fn resource_read(name: &str) -> Option<Vec<u8>> {
    let c_name = std::ffi::CString::new(name).ok()?;
    let mut ptr: *mut u8 = core::ptr::null_mut();
    let mut len: usize = 0;
    let ok = unsafe { oxide_host_resource_read(c_name.as_ptr(), &mut ptr, &mut len) };
    if ok == 0 || ptr.is_null() || len == 0 {
        return None;
    }
    let data = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    unsafe { oxide_host_string_free(ptr) };
    Some(data)
}

fn load_default_assets(
    renderer: *mut metal::MetalRenderer,
    router: &mut test_scenes::Router<MtlUploader>,
) {
    if let Some(bytes) = resource_read("fonts/Inter-Regular.ttf") {
        let _fid0 = router.text.fonts.add_font(text::Font::from_bytes(bytes));
    }
    let (w, h, data) = match resource_read("images/sample.png")
        .and_then(|png_bytes| decode_png_rgba(&png_bytes).ok())
    {
        Some(tuple) => tuple,
        None => gen_checker_rgba(256, 256),
    };
    let tex = unsafe { (*renderer).image_create_rgba8(w, h, &data, (w as usize) * 4) };
    router.set_zoom_image(tex, w, h);
    router.nine_slice_set_image(tex);
}

#[no_mangle]
pub extern "C" fn oxide_host_app_init(w: u32, h: u32, scale: f32) -> ::libc::c_int {
    #[cfg(target_os = "ios")]
    let mut perm_manager_for_subs: Option<Arc<PermissionManager>> = None;
    let mut app = app_state().lock().expect("app_state mutex");
    if app.inited {
        return 0;
    }
    app.sensors = None;
    app.networking = None;
    app.network_metrics = None;
    app.telemetry = None;
    app.telemetry_ops = None;
    #[cfg(target_os = "ios")]
    {
        app.sensor_runtime = None;
        app.network_runtime = None;
    }
    let telemetry = if app.benchmark_mode {
        None
    } else {
        let telemetry = Arc::new(TelemetryHub::new());
        app.telemetry = Some(Arc::clone(&telemetry));
        app.telemetry_ops = Some(TelemetryOperations::new(Arc::clone(&telemetry)));
        Some(telemetry)
    };
    let renderer_cfg = metal::MetalRendererConfig {
        wants_hdr: false,
        sample_count: 1,
        camera_render_mode: app.camera_render_mode,
        camera_texture_source: app.camera_texture_source,
        direct_preview_only: app.benchmark_mode,
    };
    let mut renderer = match metal::MetalRenderer::new_with_config(renderer_cfg) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("[Oxide] MetalRenderer::new_with_config() failed: {err}");
            return -1;
        }
    };
    if !app.benchmark_mode {
        let _ = renderer.resize(w, h, scale);
    }
    let mut boxed = Box::new(renderer);
    let renderer_ptr: *mut metal::MetalRenderer = &mut *boxed;
    let mut router = if app.benchmark_mode {
        None
    } else {
        Some(test_scenes::Router::new(MtlUploader { renderer: renderer_ptr }))
    };
    let sensor_bridge = if app.benchmark_mode {
        None
    } else {
        let sensor_bridge = Arc::new(SensorBridge::with_default_clock());
        router
            .as_mut()
            .expect("router available outside benchmark mode")
            .sensors_bind(&sensor_bridge);
        if let Some(telemetry) = telemetry.as_ref() {
            router
                .as_mut()
                .expect("router available outside benchmark mode")
                .telemetry_bind(telemetry);
            telemetry.update_sensors(Some(sensor_bridge.snapshot()));
        }
        Some(sensor_bridge)
    };
    let reachability = if app.benchmark_mode {
        None
    } else {
        Some(Arc::new(ReachabilityManager::with_default_clock()))
    };
    #[cfg(target_os = "ios")]
    {
        if app.benchmark_mode {
            ios_log("oxide.host-ios: benchmark mode skipping iOS runtime services");
        } else {
            let manager: Arc<dyn CameraManager + Send + Sync> = Arc::new(IosCameraManager);
            router
                .as_mut()
                .expect("router available outside benchmark mode")
                .camera_attach_manager(manager);
            let perm_iface: Arc<dyn Permissions + Send + Sync> = Arc::new(IosPermissions);
            let perm_manager = Arc::new(PermissionManager::with_default_clock(perm_iface));
            router
                .as_mut()
                .expect("router available outside benchmark mode")
                .permissions_bind(&perm_manager);
            let snapshot = initialize_permissions(&perm_manager);
            router
                .as_mut()
                .expect("router available outside benchmark mode")
                .permissions_update(&snapshot);
            app.permission_states = snapshot;
            app.permissions = Some(Arc::clone(&perm_manager));
            app.permission_subs.clear();
            if let Some(telemetry) = telemetry.as_ref() {
                telemetry.update_permissions(app.permission_states.clone());
            }
            let runtime = SensorRuntime::new(
                Arc::clone(
                    sensor_bridge.as_ref().expect("sensor bridge available outside benchmark mode"),
                ),
                Arc::clone(&perm_manager),
                Arc::downgrade(
                    telemetry.as_ref().expect("telemetry available outside benchmark mode"),
                ),
                None,
            );
            app.sensor_runtime = Some(runtime);
            match NetworkRuntime::new(
                Arc::clone(
                    reachability.as_ref().expect("reachability available outside benchmark mode"),
                ),
                Arc::clone(telemetry.as_ref().expect("telemetry available outside benchmark mode")),
            ) {
                Ok(net_runtime) => app.network_runtime = Some(net_runtime),
                Err(err) => ios_log(&format!("reachability start failed: {err}")),
            }
            perm_manager_for_subs = Some(Arc::clone(&perm_manager));
        }
    }
    app.sensors = sensor_bridge.clone();
    app.networking = reachability.clone();
    if let Some(router) = router.as_mut() {
        load_default_assets(renderer_ptr, router);
    }
    let default_damage_use = 0.70f32;
    let default_damage_pref = 0.25f32;
    if let Some(router) = router.as_mut() {
        router.damage_set_options(false, default_damage_use, default_damage_pref);
    }
    unsafe {
        (*renderer_ptr).set_damage_options(false, default_damage_use, default_damage_pref);
    }
    app.last_ms = timing::now_ms();
    app.last_stats = StatsSnapshot::default();
    app.window = WindowMetrics {
        width_dp: (w as f32) / scale.max(1.0),
        height_dp: (h as f32) / scale.max(1.0),
        scale,
        safe_left: 0.0,
        safe_top: 0.0,
        safe_right: 0.0,
        safe_bottom: 0.0,
    };
    app.touch = PrimaryTouchTracker::default();
    app.space_down = false;
    app.memory_warnings = 0;

    let desired_overlay = if app.overlay_dirty { app.overlay_visible } else { !app.benchmark_mode };
    if !desired_overlay {
        if let Some(router) = router.as_mut() {
            router.toggle_overlay();
        }
    }
    app.overlay_visible = desired_overlay;
    app.overlay_dirty = false;

    let desired_reduce = if app.reduce_motion_dirty { app.reduce_motion_on } else { false };
    if desired_reduce {
        if let Some(router) = router.as_mut() {
            router.set_reduce_motion(true);
        }
    }
    app.reduce_motion_on = desired_reduce;
    app.reduce_motion_dirty = false;
    app.snapshot_status.clear();
    app.camera_running = false;
    app.router = router;
    app.renderer = Some(boxed);
    if let Some(ops) = app.telemetry_ops.as_ref() {
        ops.handle_foreground(timing::now_ms());
    }
    process_telemetry_commands_locked(&mut app);
    if !app.benchmark_mode {
        oxide_host_set_window_resized_callback(Some(window_resized_cb));
        oxide_host_set_touch_callback(Some(touch_cb));
        oxide_host_set_pointer_callback(Some(pointer_cb));
        oxide_host_set_key_callback(Some(key_cb));
        oxide_host_set_text_commit_callback(Some(text_commit_cb));
        oxide_host_set_text_composition_callback(Some(text_composition_cb));
        oxide_host_set_text_selection_callback(Some(text_selection_cb));
        oxide_host_set_ime_callbacks(Some(ime_shown_cb), Some(ime_hidden_cb));
    }
    app.inited = true;
    #[cfg(target_os = "ios")]
    let manager_to_subscribe = perm_manager_for_subs.take();
    drop(app);
    #[cfg(target_os = "ios")]
    if let Some(manager) = manager_to_subscribe {
        install_permission_subscriptions(manager);
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_app_frame(w: u32, h: u32, scale: f32) -> ::libc::c_int {
    oxide_host_app_frame_inner(w, h, scale, core::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn oxide_host_camera_preview_plan(w: u32, h: u32, scale: f32) -> ::libc::c_int {
    let app = app_state().lock().expect("app_state mutex");
    if !app.inited {
        return -1;
    }
    if !benchmark_camera_fast_path_active(&app) {
        return 1;
    }
    let Some(renderer) = app.renderer.as_ref() else {
        return -2;
    };
    camera_preview_plan(renderer, app.camera_running, w, h, scale)
}

#[no_mangle]
pub extern "C" fn oxide_host_camera_preview_plan_reason(
    w: u32,
    h: u32,
    scale: f32,
) -> ::libc::c_int {
    let app = app_state().lock().expect("app_state mutex");
    if !app.inited {
        return -1;
    }
    if !benchmark_camera_fast_path_active(&app) {
        return metal::CAMERA_PREVIEW_REASON_NON_DIRECT_PREVIEW as ::libc::c_int;
    }
    let Some(renderer) = app.renderer.as_ref() else {
        return -2;
    };
    camera_preview_plan_reason(renderer, app.camera_running, w, h, scale)
}

#[no_mangle]
pub extern "C" fn oxide_host_app_frame_with_drawable(
    w: u32,
    h: u32,
    scale: f32,
    drawable_ptr: *mut ::libc::c_void,
) -> ::libc::c_int {
    oxide_host_app_frame_inner(w, h, scale, drawable_ptr)
}

fn oxide_host_app_frame_inner(
    w: u32,
    h: u32,
    scale: f32,
    drawable_ptr: *mut ::libc::c_void,
) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    if !app.inited {
        return -1;
    }
    if benchmark_camera_fast_path_active(&app) {
        let Some(mut renderer) = app.renderer.take() else {
            return -2;
        };
        let camera_running = app.camera_running;
        drop(app);

        let render_result = render_camera_benchmark_fast_path(
            &mut renderer,
            camera_running,
            w,
            h,
            scale,
            drawable_ptr,
        );

        let mut app = app_state().lock().expect("app_state mutex");
        debug_assert!(app.renderer.is_none(), "benchmark fast path renderer unexpectedly replaced");
        app.renderer = Some(renderer);
        match render_result {
            Ok(stats) => {
                app.last_stats = stats;
                if ios_log_enabled() {
                    ios_log("app_frame: camera fast path ok");
                }
                return 0;
            }
            Err(code) => return code,
        }
    }
    process_telemetry_commands_locked(&mut app);
    if ios_log_enabled() {
        ios_log(&format!("app_frame: w={} h={} scale={:.2}", w, h, scale));
    }
    let now = timing::now_ms();
    let dt_ms = (now.saturating_sub(app.last_ms)) as u32;
    app.last_ms = now;
    #[cfg(target_os = "ios")]
    {
        if let Some(runtime) = app.network_runtime.as_mut() {
            runtime.tick(now);
            let metrics = runtime.metrics();
            if let Some(telemetry) = app.telemetry.as_ref() {
                telemetry.update_network_metrics(Some(metrics.clone()));
            }
            app.network_metrics = Some(metrics);
        }
    }
    process_telemetry_commands_locked(&mut app);
    {
        let renderer = match app.renderer.as_mut().map(|b| b.as_mut()) {
            Some(r) => r,
            None => {
                return -2;
            }
        };
        if !drawable_ptr.is_null() {
            let present_result = unsafe {
                with_perf_signpost("camera.host.present", || {
                    renderer.prepare_present_drawable(drawable_ptr.cast())
                })
            };
            if present_result.is_err() {
                return -5;
            }
        }
        let _ = with_perf_signpost("camera.renderer.resize", || renderer.resize(w, h, scale));
    }
    let mut builder = ui::DrawListBuilder::new();
    let vp =
        gfx_api::RectF::new(0.0, 0.0, (w as f32) / scale.max(1.0), (h as f32) / scale.max(1.0));
    let router_update = with_perf_signpost("camera.router.update_draw", || {
        let router = match app.router.as_mut() {
            Some(r) => r,
            None => return None,
        };
        router.update(now, dt_ms);
        router.draw(vp, scale, &mut builder);
        let draws = router.counters.draws.min(u32::MAX as usize) as u32;
        let anims = router.counters.anims.min(u32::MAX as usize) as u32;
        let stats = StatsSnapshot {
            fps: router.counters.fps,
            draws,
            anims,
            damage_pct: 0.0,
            damage_rects: 0,
            ..StatsSnapshot::default()
        };
        Some((router.take_damage(), stats))
    });
    let (damage_rects, stats) = match router_update {
        Some(value) => value,
        None => {
            if !drawable_ptr.is_null() {
                if let Some(renderer) = app.renderer.as_mut().map(|b| b.as_mut()) {
                    let _ = renderer.cancel_present_drawable();
                }
            }
            return -3;
        }
    };
    let perf_stats = {
        let renderer = match app.renderer.as_mut().map(|b| b.as_mut()) {
            Some(r) => r,
            None => return -2,
        };
        let damage = gfx_api::Damage { rects: damage_rects };
        let token = with_perf_signpost("camera.renderer.begin_frame", || {
            renderer.begin_frame(&gfx_api::FrameTarget, Some(&damage))
        });
        if builder.drawlist().items.len() > 1 {
            with_perf_signpost("camera.renderer.coalesce", || {
                let dl = builder.drawlist_mut();
                oxide_ui_core::coalesce_adjacent_draws(dl);
            });
        }
        with_perf_signpost("camera.renderer.encode_pass", || {
            renderer.encode_pass(builder.drawlist());
        });
        if with_perf_signpost("camera.renderer.submit", || renderer.submit(token)).is_err() {
            if !drawable_ptr.is_null() {
                let _ = renderer.cancel_present_drawable();
            }
            return -4;
        }
        renderer.last_stats()
    };
    let paused = perf_stats.cam_paused != 0 || !app.camera_running;
    let running = app.camera_running;
    let overlay_visible = app.overlay_visible;
    if let Some(router) = app.router.as_mut() {
        router.damage_set_stats(perf_stats.damage_pct, perf_stats.damage_rects);
        if overlay_visible {
            let metrics = test_scenes::CameraMetrics {
                width: perf_stats.cam_width,
                height: perf_stats.cam_height,
                bit_depth: perf_stats.cam_bit_depth,
                matrix: perf_stats.cam_matrix,
                video_range: perf_stats.cam_video_range,
                color_space: perf_stats.cam_color_space,
                coverage_pct: perf_stats.cam_coverage_pct,
                blur_ms: perf_stats.blur_ms as f32,
                blur_updates: perf_stats.blur_updates,
                update_period_ms: perf_stats.blur_period_ms,
                paused,
                running,
                low_power: perf_stats.low_power != 0,
                thermal: perf_stats.thermal,
                fps: if perf_stats.blur_period_ms > 0 {
                    1000.0 / (perf_stats.blur_period_ms as f32)
                } else {
                    0.0
                },
            };
            router.camera_set_metrics(metrics);
        }
    }
    let mut stats = stats;
    stats.damage_pct = perf_stats.damage_pct;
    stats.damage_rects = perf_stats.damage_rects;
    stats.cam_coverage_pct = perf_stats.cam_coverage_pct;
    stats.cam_blur_ms = perf_stats.blur_ms as f32;
    stats.cam_blur_updates = perf_stats.blur_updates;
    stats.cam_update_period_ms = perf_stats.blur_period_ms;
    stats.cam_paused = if paused { 1 } else { 0 };
    stats.cam_low_power = perf_stats.low_power;
    stats.cam_thermal = perf_stats.thermal;
    stats.cam_width = perf_stats.cam_width;
    stats.cam_height = perf_stats.cam_height;
    stats.cam_bit_depth = perf_stats.cam_bit_depth;
    stats.cam_matrix = perf_stats.cam_matrix;
    stats.cam_video_range = perf_stats.cam_video_range;
    stats.cam_color_space = perf_stats.cam_color_space;
    stats.cam_running = if app.camera_running { 1 } else { 0 };
    stats.cam_fps = if stats.cam_update_period_ms > 0 {
        1000.0 / (stats.cam_update_period_ms as f32)
    } else {
        0.0
    };
    apply_camera_stage_perf(&mut stats, &perf_stats);
    if app.camera_running {
        apply_camera_capture_perf(&mut stats);
    }
    app.last_stats = stats;
    ios_log("app_frame: ok");
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_app_did_enter_background() {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_background(timing::now_ms());
        }
        process_telemetry_commands_locked(app);
    });
}

#[no_mangle]
pub extern "C" fn oxide_host_app_will_enter_foreground() {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_foreground(timing::now_ms());
        }
        process_telemetry_commands_locked(app);
    });
}

#[no_mangle]
pub extern "C" fn oxide_host_app_will_terminate() {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_shutdown(timing::now_ms());
        }
        process_telemetry_commands_locked(app);
    });
}

#[no_mangle]
pub extern "C" fn oxide_host_on_memory_warning() {
    with_app_mut(|app| {
        app.memory_warnings = app.memory_warnings.saturating_add(1);
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_memory_pressure(timing::now_ms(), MemoryPressureLevel::Critical);
        }
        process_telemetry_commands_locked(app);
    })
    .unwrap_or_else(|| {
        eprintln!("[Oxide] memory warning received before app init");
    });
}

// ===== Camera options control (for UITests) =====

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_options(
    blur: u8,
    sigma: f32,
    grayscale: u8,
    animate: u8,
) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    if app.benchmark_mode {
        return 0;
    }
    let router = match app.router.as_mut() {
        Some(r) => r,
        None => return -1,
    };
    let b = blur != 0;
    let g = grayscale != 0;
    let a = animate != 0;
    router.camera_set_options(b, sigma, g, a);
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_render_mode(mode: i32) -> ::libc::c_int {
    let mode = match mode {
        1 => metal::CameraRenderMode::Nv12Legacy,
        2 => metal::CameraRenderMode::BgraBenchmark,
        _ => metal::CameraRenderMode::Nv12Optimized,
    };
    #[cfg(target_os = "ios")]
    let _ = unsafe {
        oxide_cam_set_preview_pixel_format(
            if matches!(mode, metal::CameraRenderMode::BgraBenchmark) { 1 } else { 0 },
        )
    };
    let mut app = app_state().lock().expect("app_state mutex");
    app.camera_render_mode = mode;
    if let Some(renderer) = app.renderer.as_mut() {
        renderer.set_camera_render_mode(mode);
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_texture_source(source: i32) -> ::libc::c_int {
    let source = match source {
        1 => metal::CameraTextureSource::SyntheticBenchmark,
        _ => metal::CameraTextureSource::Live,
    };
    let mut app = app_state().lock().expect("app_state mutex");
    app.camera_texture_source = source;
    if let Some(renderer) = app.renderer.as_mut() {
        renderer.set_camera_texture_source(source);
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_running(on: u8) -> ::libc::c_int {
    oxide_host_set_camera_running_mode(on, 0)
}

#[no_mangle]
pub extern "C" fn oxide_host_reset_camera_perf_counters() -> ::libc::c_int {
    #[cfg(target_os = "ios")]
    unsafe {
        return oxide_cam_reset_perf_counters();
    }
    #[cfg(not(target_os = "ios"))]
    {
        0
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_running_mode(on: u8, _preview_only: u8) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let want_on = on != 0;
    if want_on && !app.camera_running {
        #[cfg(target_os = "ios")]
        let rc = unsafe {
            if _preview_only != 0 {
                oxide_cam_start_default_preview_only()
            } else {
                oxide_cam_start_default()
            }
        };
        #[cfg(not(target_os = "ios"))]
        let rc = 0;
        if rc == 0 {
            app.camera_running = true;
            app.last_stats.cam_running = 1;
            app.last_stats.cam_paused = 0;
        } else {
            app.camera_running = false;
            app.last_stats.cam_running = 0;
        }
        rc
    } else if !want_on && app.camera_running {
        #[cfg(target_os = "ios")]
        unsafe {
            oxide_cam_stop();
        }
        app.camera_running = false;
        app.last_stats.cam_running = 0;
        app.last_stats.cam_paused = 1;
        0
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_anim_play(play: u8) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let router = match app.router.as_mut() {
        Some(r) => r,
        None => return -1,
    };
    router.anim_set_play(play != 0);
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_anim_progress(progress: f32) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let router = match app.router.as_mut() {
        Some(r) => r,
        None => return -1,
    };
    router.anim_set_progress(progress);
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_damage_options(
    enabled: u8,
    use_thresh: f32,
    prefilter: f32,
) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let enabled_bool = enabled != 0;
    if let Some(router) = app.router.as_mut() {
        router.damage_set_options(enabled_bool, use_thresh, prefilter);
    } else {
        return -1;
    }
    if let Some(renderer) = app.renderer.as_mut().map(|b| b.as_mut()) {
        renderer.set_damage_options(enabled_bool, use_thresh, prefilter);
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_nine_slice(slice_px: f32, alpha: f32) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let router = match app.router.as_mut() {
        Some(r) => r,
        None => return -1,
    };
    router.nine_slice_set_options(slice_px, alpha);
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_sdf_font(font_px: f32) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let router = match app.router.as_mut() {
        Some(r) => r,
        None => return -1,
    };
    router.sdf_set_font_px(font_px);
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_take_snapshot() -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    let AppState { renderer, router, snapshot_status, .. } = &mut *app;
    let router = match router.as_mut() {
        Some(rt) => rt,
        None => {
            let msg = "Snapshot failed: router unavailable".to_owned();
            *snapshot_status = msg.clone();
            ios_log(&msg);
            return -2;
        }
    };
    let renderer = match renderer.as_mut().map(|b| b.as_mut()) {
        Some(r) => r,
        None => {
            let msg = "Snapshot failed: renderer unavailable".to_owned();
            router.readback_set_status(msg.clone());
            *snapshot_status = msg.clone();
            ios_log(&msg);
            return -1;
        }
    };

    let outcome: Result<(std::path::PathBuf, u32, u32), String> = (|| {
        let (w, h, mut rgba) =
            renderer.readback_bgra8().ok_or_else(|| "no target texture".to_owned())?;
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let mut path = std::env::temp_dir();
        let filename = format!("oxide-snapshot-{}-{}x{}.png", timing::now_ms(), w, h);
        path.push(filename);
        let mut file = File::create(&path).map_err(|err| format!("create file: {err}"))?;
        let mut encoder = png::Encoder::new(&mut file, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        {
            let mut writer =
                encoder.write_header().map_err(|err| format!("encode header: {err}"))?;
            writer.write_image_data(&rgba).map_err(|err| format!("write data: {err}"))?;
        }
        file.flush().map_err(|err| format!("flush: {err}"))?;
        Ok((path, w, h))
    })();

    let (message, rc) = match outcome {
        Ok((path, w, h)) => {
            let msg = format!("Saved snapshot {}x{} to {}", w, h, path.display());
            (msg, 0)
        }
        Err(err) => {
            let msg = format!("Snapshot failed: {err}");
            (msg, -3)
        }
    };

    router.readback_set_status(message.clone());
    *snapshot_status = message.clone();
    ios_log(&message);
    rc
}

#[no_mangle]
pub extern "C" fn oxide_host_get_snapshot_status(
    out_ptr: *mut ::libc::c_char,
    out_len: u32,
) -> u32 {
    if out_ptr.is_null() || out_len == 0 {
        return 0;
    }
    let message = {
        let app = app_state().lock().expect("app_state mutex");
        if let Some(router) = app.router.as_ref() {
            router.readback_status().to_owned()
        } else {
            app.snapshot_status.clone()
        }
    };
    let bytes = message.as_bytes();
    let max_copy = (out_len as usize).saturating_sub(1);
    let copy_len = bytes.len().min(max_copy);
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr as *mut u8, copy_len);
        *out_ptr.add(copy_len) = 0;
    }
    bytes.len() as u32
}

#[no_mangle]
pub extern "C" fn oxide_host_input_log(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(message) = std::str::from_utf8(slice) else { return };
    let owned = message.to_owned();
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_log(&owned);
        }
    });
}

#[no_mangle]
pub extern "C" fn oxide_host_app_shutdown() {
    if let Some(state) = APP_STATE.get() {
        let mut app = state.lock().expect("app_state mutex");
        oxide_host_set_window_resized_callback(None);
        oxide_host_set_touch_callback(None);
        oxide_host_set_pointer_callback(None);
        oxide_host_set_key_callback(None);
        oxide_host_set_text_commit_callback(None);
        oxide_host_set_text_composition_callback(None);
        oxide_host_set_text_selection_callback(None);
        oxide_host_set_ime_callbacks(None, None);
        if let Some(router) = app.router.as_mut() {
            router.camera_detach_manager();
        }
        app.renderer = None;
        app.router = None;
        app.benchmark_scene_index = benchmark_camera_scene_index();
        app.snapshot_status.clear();
        app.inited = false;
        app.benchmark_mode = false;
        app.last_ms = 0;
        app.overlay_visible = true;
        app.overlay_dirty = false;
        app.reduce_motion_on = false;
        app.reduce_motion_dirty = false;
        app.space_down = false;
        app.camera_running = false;
        app.sensors = None;
        app.networking = None;
        app.network_metrics = None;
        app.telemetry = None;
        app.telemetry_ops = None;
        #[cfg(target_os = "ios")]
        {
            app.sensor_runtime = None;
            app.network_runtime = None;
        }
        app.permissions = None;
        app.permission_subs.clear();
        app.permission_states.clear();
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_benchmark_mode(on: u8) -> ::libc::c_int {
    let mut app = app_state().lock().expect("app_state mutex");
    if app.inited {
        return -1;
    }
    app.benchmark_mode = on != 0;
    0
}

extern "C" fn window_resized_cb(
    w: f32,
    h: f32,
    scale: f32,
    safe_l: f32,
    safe_t: f32,
    safe_r: f32,
    safe_b: f32,
) {
    let _ = with_app_mut(|app| {
        app.window = WindowMetrics {
            width_dp: w,
            height_dp: h,
            scale,
            safe_left: safe_l,
            safe_top: safe_t,
            safe_right: safe_r,
            safe_bottom: safe_b,
        };
    });
}

extern "C" fn pointer_cb(x: f32, y: f32, dx: f32, dy: f32, buttons: u32, _mods: u32, _ts: u64) {
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_pointer(x, y, dx, dy, buttons);
        }
    });
}

extern "C" fn touch_cb(
    id: u64,
    phase: u32,
    x: f32,
    y: f32,
    _pressure: f32,
    _has_pressure: u8,
    _tilt_alt: f32,
    _tilt_azi: f32,
    _has_tilt: u8,
    _device: u32,
    ts_ns: u64,
) {
    let _ = with_app_mut(|app| {
        let result = app.touch.on_event(id, phase, x, y, ts_ns);
        if let Some(router) = app.router.as_mut() {
            if let Some(ptr) = result.pointer {
                router.input_pointer(ptr.x, ptr.y, ptr.dx, ptr.dy, ptr.buttons);
            }
            if result.double_tap {
                router.input_double_tap();
            }
        }
    });
}

extern "C" fn key_cb(
    code: u32,
    chars_ptr: *const u8,
    chars_len: usize,
    repeat: u8,
    _mods: u32,
    _ts: u64,
) {
    let chars = unsafe {
        if chars_ptr.is_null() || chars_len == 0 {
            ""
        } else {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(chars_ptr, chars_len))
        }
    };
    let ch = chars.chars().next();
    let is_up = repeat == 2;
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            if let Some(ch) = ch {
                match ch {
                    '1'..='5' => {
                        if !is_up {
                            router.key_scene_select((ch as u8 - b'1') as usize);
                        }
                    }
                    ' ' => {
                        if is_up {
                            if app.space_down {
                                router.key_space_up();
                                app.space_down = false;
                            }
                        } else if !app.space_down {
                            router.key_space_down();
                            app.space_down = true;
                        }
                    }
                    'f' | 'F' => {
                        if !is_up {
                            router.toggle_overlay();
                            app.overlay_visible = !app.overlay_visible;
                            app.overlay_dirty = false;
                        }
                    }
                    'm' | 'M' => {
                        if !is_up {
                            app.reduce_motion_on = !app.reduce_motion_on;
                            router.set_reduce_motion(app.reduce_motion_on);
                            app.reduce_motion_dirty = false;
                        }
                    }
                    'z' | 'Z' => {
                        if !is_up {
                            router.key_zoom_reset();
                        }
                    }
                    _ => {}
                }
            }
            if !is_up {
                match code {
                    123 | 0x50 => router.key_arrow_left(),
                    124 | 0x4F => router.key_arrow_right(),
                    125 | 0x51 => router.key_arrow_down(),
                    126 | 0x52 => router.key_arrow_up(),
                    _ => {}
                }
            }
        }
    });
}

extern "C" fn text_commit_cb(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(text) = std::str::from_utf8(slice) else { return };
    let owned = text.to_owned();
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_commit(&owned);
        }
    });
}

extern "C" fn text_composition_cb(start: u32, end: u32, ptr: *const u8, len: usize) {
    let text = if ptr.is_null() || len == 0 {
        String::new()
    } else {
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_owned(),
            Err(_) => String::new(),
        }
    };
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_set_composition(start, end, &text);
        }
    });
}

extern "C" fn text_selection_cb(start: u32, end: u32) {
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_set_selection(start, end);
        }
    });
}

extern "C" fn ime_shown_cb(x: f32, y: f32, w: f32, h: f32) {
    let rect = gfx_api::RectF::new(x, y, w, h);
    let message = format!("IME shown at ({:.0},{:.0}) {:.0}x{:.0}", rect.x, rect.y, rect.w, rect.h);
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_set_ime_rect(rect);
            router.input_log(&message);
        }
    });
}

extern "C" fn ime_hidden_cb() {
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_hide_ime();
            router.input_log("IME hidden");
        }
    });
}

#[repr(C)]
pub struct OxideHostStats {
    pub fps: f32,
    pub draws: u32,
    pub anims: u32,
    pub memory_warnings: u32,
    pub damage_pct: f32,
    pub damage_rects: u32,
    pub cam_coverage_pct: f32,
    pub cam_blur_ms: f32,
    pub cam_blur_updates: u32,
    pub cam_update_period_ms: u32,
    pub cam_paused: u8,
    pub cam_low_power: u8,
    pub cam_thermal: u8,
    pub cam_width: u32,
    pub cam_height: u32,
    pub cam_bit_depth: u8,
    pub cam_matrix: u8,
    pub cam_video_range: u8,
    pub cam_color_space: u8,
    pub cam_running: u8,
    pub cam_fps: f32,
    pub cam_poll_submissions_ms: f32,
    pub cam_fetch_ms: f32,
    pub cam_setup_ms: f32,
    pub cam_encode_quad_ms: f32,
    pub cam_command_buffer_ms: f32,
    pub cam_encoder_ms: f32,
    pub cam_encode_bind_ms: f32,
    pub cam_encode_draw_ms: f32,
    pub cam_end_encoding_ms: f32,
    pub cam_present_ms: f32,
    pub cam_commit_ms: f32,
    pub cam_gpu_ms: f32,
    pub cam_capture_total_ms: f32,
    pub cam_capture_sample_setup_ms: f32,
    pub cam_capture_lock_ms: f32,
    pub cam_capture_texture_bridge_ms: f32,
    pub cam_capture_publish_ms: f32,
    pub cam_capture_publish_lock_ms: f32,
    pub cam_capture_publish_texture_refs_ms: f32,
    pub cam_capture_publish_pixel_buffer_ms: f32,
    pub cam_capture_frame_delivery_ms: f32,
    pub cam_sample_delivery_pool_bytes: u64,
    pub cam_sample_delivery_pool_surfaces: u32,
    pub cam_active_sample_surface_bytes: u64,
    pub cam_active_sample_surface_surfaces: u32,
    pub cam_active_sample_buffers: u32,
    pub cam_peak_active_sample_surface_bytes: u64,
    pub cam_peak_active_sample_surface_surfaces: u32,
    pub cam_peak_active_sample_buffers: u32,
    pub cam_sample_delivery_total_samples: u32,
    pub cam_sample_delivery_reused_frames: u32,
    pub cam_sample_delivery_reused_surfaces: u32,
    pub cam_sample_delivery_max_reuse_gap_frames: u32,
    pub cam_retained_sample_surface_bytes: u64,
    pub cam_retained_sample_surface_surfaces: u32,
    pub cam_retained_published_slot_surface_bytes: u64,
    pub cam_retained_published_slot_surfaces: u32,
    pub cam_retained_latest_pixel_buffer_surface_bytes: u64,
    pub cam_retained_latest_pixel_buffer_surface_surfaces: u32,
    pub cam_latest_published_generation: u64,
    pub cam_latest_published_timestamp_ns: u64,
    pub renderer_memory_total_bytes: u64,
    pub renderer_memory_draw_targets_bytes: u64,
    pub renderer_memory_draw_target_main_bytes: u64,
    pub renderer_memory_draw_target_msaa_bytes: u64,
    pub renderer_memory_effect_targets_bytes: u64,
    pub renderer_memory_effect_prepass_bytes: u64,
    pub renderer_memory_effect_blur_chain_bytes: u64,
    pub renderer_memory_live_camera_bytes: u64,
    pub renderer_memory_camera_cache_bytes: u64,
    pub renderer_memory_camera_blur_cache_bytes: u64,
    pub renderer_memory_camera_transition_cache_bytes: u64,
    pub renderer_memory_benchmark_camera_bytes: u64,
    pub renderer_memory_layer_cache_bytes: u64,
    pub renderer_memory_image_cache_bytes: u64,
    pub renderer_memory_buffer_bytes: u64,
    pub renderer_pending_command_buffers: u32,
    pub renderer_pending_present_drawables: u32,
    pub renderer_pending_present_textures: u32,
    pub renderer_preview_submission_depth: u32,
    pub renderer_preview_submission_skipped: u32,
    pub renderer_preview_submission_frame_age_ms: f32,
}

#[no_mangle]
pub extern "C" fn oxide_host_app_stats(out: *mut OxideHostStats) -> ::libc::c_int {
    if out.is_null() {
        return -1;
    }
    if let Some(state) = APP_STATE.get() {
        if let Ok(app) = state.lock() {
            let mut snap = app.last_stats;
            if benchmark_camera_fast_path_active(&app) && app.camera_running {
                apply_camera_capture_perf(&mut snap);
                apply_camera_contract_snapshot(&mut snap);
            }
            unsafe {
                *out = OxideHostStats {
                    fps: snap.fps,
                    draws: snap.draws,
                    anims: snap.anims,
                    memory_warnings: app.memory_warnings,
                    damage_pct: snap.damage_pct,
                    damage_rects: snap.damage_rects,
                    cam_coverage_pct: snap.cam_coverage_pct,
                    cam_blur_ms: snap.cam_blur_ms,
                    cam_blur_updates: snap.cam_blur_updates,
                    cam_update_period_ms: snap.cam_update_period_ms,
                    cam_paused: snap.cam_paused,
                    cam_low_power: snap.cam_low_power,
                    cam_thermal: snap.cam_thermal,
                    cam_width: snap.cam_width,
                    cam_height: snap.cam_height,
                    cam_bit_depth: snap.cam_bit_depth,
                    cam_matrix: snap.cam_matrix,
                    cam_video_range: snap.cam_video_range,
                    cam_color_space: snap.cam_color_space,
                    cam_running: snap.cam_running,
                    cam_fps: snap.cam_fps,
                    cam_poll_submissions_ms: snap.cam_poll_submissions_ms,
                    cam_fetch_ms: snap.cam_fetch_ms,
                    cam_setup_ms: snap.cam_setup_ms,
                    cam_encode_quad_ms: snap.cam_encode_quad_ms,
                    cam_command_buffer_ms: snap.cam_command_buffer_ms,
                    cam_encoder_ms: snap.cam_encoder_ms,
                    cam_encode_bind_ms: snap.cam_encode_bind_ms,
                    cam_encode_draw_ms: snap.cam_encode_draw_ms,
                    cam_end_encoding_ms: snap.cam_end_encoding_ms,
                    cam_present_ms: snap.cam_present_ms,
                    cam_commit_ms: snap.cam_commit_ms,
                    cam_gpu_ms: snap.cam_gpu_ms,
                    cam_capture_total_ms: snap.cam_capture_total_ms,
                    cam_capture_sample_setup_ms: snap.cam_capture_sample_setup_ms,
                    cam_capture_lock_ms: snap.cam_capture_lock_ms,
                    cam_capture_texture_bridge_ms: snap.cam_capture_texture_bridge_ms,
                    cam_capture_publish_ms: snap.cam_capture_publish_ms,
                    cam_capture_publish_lock_ms: snap.cam_capture_publish_lock_ms,
                    cam_capture_publish_texture_refs_ms: snap.cam_capture_publish_texture_refs_ms,
                    cam_capture_publish_pixel_buffer_ms: snap.cam_capture_publish_pixel_buffer_ms,
                    cam_capture_frame_delivery_ms: snap.cam_capture_frame_delivery_ms,
                    cam_sample_delivery_pool_bytes: snap.cam_sample_delivery_pool_bytes,
                    cam_sample_delivery_pool_surfaces: snap.cam_sample_delivery_pool_surfaces,
                    cam_active_sample_surface_bytes: snap.cam_active_sample_surface_bytes,
                    cam_active_sample_surface_surfaces: snap.cam_active_sample_surface_surfaces,
                    cam_active_sample_buffers: snap.cam_active_sample_buffers,
                    cam_peak_active_sample_surface_bytes: snap.cam_peak_active_sample_surface_bytes,
                    cam_peak_active_sample_surface_surfaces: snap
                        .cam_peak_active_sample_surface_surfaces,
                    cam_peak_active_sample_buffers: snap.cam_peak_active_sample_buffers,
                    cam_sample_delivery_total_samples: snap.cam_sample_delivery_total_samples,
                    cam_sample_delivery_reused_frames: snap.cam_sample_delivery_reused_frames,
                    cam_sample_delivery_reused_surfaces: snap.cam_sample_delivery_reused_surfaces,
                    cam_sample_delivery_max_reuse_gap_frames: snap
                        .cam_sample_delivery_max_reuse_gap_frames,
                    cam_retained_sample_surface_bytes: snap.cam_retained_sample_surface_bytes,
                    cam_retained_sample_surface_surfaces: snap.cam_retained_sample_surface_surfaces,
                    cam_retained_published_slot_surface_bytes: snap
                        .cam_retained_published_slot_surface_bytes,
                    cam_retained_published_slot_surfaces: snap.cam_retained_published_slot_surfaces,
                    cam_retained_latest_pixel_buffer_surface_bytes: snap
                        .cam_retained_latest_pixel_buffer_surface_bytes,
                    cam_retained_latest_pixel_buffer_surface_surfaces: snap
                        .cam_retained_latest_pixel_buffer_surface_surfaces,
                    cam_latest_published_generation: snap.cam_latest_published_generation,
                    cam_latest_published_timestamp_ns: snap.cam_latest_published_timestamp_ns,
                    renderer_memory_total_bytes: snap.renderer_memory_total_bytes,
                    renderer_memory_draw_targets_bytes: snap.renderer_memory_draw_targets_bytes,
                    renderer_memory_draw_target_main_bytes: snap
                        .renderer_memory_draw_target_main_bytes,
                    renderer_memory_draw_target_msaa_bytes: snap
                        .renderer_memory_draw_target_msaa_bytes,
                    renderer_memory_effect_targets_bytes: snap.renderer_memory_effect_targets_bytes,
                    renderer_memory_effect_prepass_bytes: snap.renderer_memory_effect_prepass_bytes,
                    renderer_memory_effect_blur_chain_bytes: snap
                        .renderer_memory_effect_blur_chain_bytes,
                    renderer_memory_live_camera_bytes: snap.renderer_memory_live_camera_bytes,
                    renderer_memory_camera_cache_bytes: snap.renderer_memory_camera_cache_bytes,
                    renderer_memory_camera_blur_cache_bytes: snap
                        .renderer_memory_camera_blur_cache_bytes,
                    renderer_memory_camera_transition_cache_bytes: snap
                        .renderer_memory_camera_transition_cache_bytes,
                    renderer_memory_benchmark_camera_bytes: snap
                        .renderer_memory_benchmark_camera_bytes,
                    renderer_memory_layer_cache_bytes: snap.renderer_memory_layer_cache_bytes,
                    renderer_memory_image_cache_bytes: snap.renderer_memory_image_cache_bytes,
                    renderer_memory_buffer_bytes: snap.renderer_memory_buffer_bytes,
                    renderer_pending_command_buffers: snap.renderer_pending_command_buffers,
                    renderer_pending_present_drawables: snap.renderer_pending_present_drawables,
                    renderer_pending_present_textures: snap.renderer_pending_present_textures,
                    renderer_preview_submission_depth: snap.renderer_preview_submission_depth,
                    renderer_preview_submission_skipped: snap.renderer_preview_submission_skipped,
                    renderer_preview_submission_frame_age_ms: snap
                        .renderer_preview_submission_frame_age_ms,
                };
            }
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn oxide_host_run_perf_suite(smoke: u8) -> ::libc::c_int {
    match perf_runner::collect_suite_json(smoke != 0) {
        Ok(json) => {
            if let Ok(mut slot) = perf_report_json().lock() {
                *slot = Some(json.into_bytes());
                0
            } else {
                ios_log("oxide.host-ios: perf report cache mutex poisoned");
                -2
            }
        }
        Err(err) => {
            ios_log(&format!("oxide.host-ios: collect perf suite failed: {err}"));
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_perf_report_json_len() -> usize {
    perf_report_json()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|bytes| bytes.len().saturating_add(1)))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn oxide_host_copy_perf_report_json(out_ptr: *mut u8, out_len: usize) -> usize {
    let Ok(slot) = perf_report_json().lock() else { return 0 };
    let Some(bytes) = slot.as_ref() else { return 0 };
    let needed = bytes.len().saturating_add(1);
    if !out_ptr.is_null() && out_len >= needed {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, bytes.len());
            *out_ptr.add(bytes.len()) = 0;
        }
    }
    needed
}

#[no_mangle]
pub extern "C" fn oxide_host_clear_perf_report_json() {
    if let Ok(mut slot) = perf_report_json().lock() {
        *slot = None;
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_scene_count() -> u32 {
    test_scenes::Router::<MtlUploader>::scene_names().len() as u32
}

#[no_mangle]
pub extern "C" fn oxide_host_scene_name(index: u32, out_ptr: *mut u8, out_len: usize) -> u32 {
    let names = test_scenes::Router::<MtlUploader>::scene_names();
    let Some(name) = names.get(index as usize) else { return 0 };
    let bytes = name.as_bytes();
    let needed = bytes.len().saturating_add(1);
    if !out_ptr.is_null() && out_len >= needed {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, bytes.len());
            *out_ptr.add(bytes.len()) = 0;
        }
    }
    needed as u32
}

#[no_mangle]
pub extern "C" fn oxide_host_set_scene(index: u32) -> ::libc::c_int {
    let count = oxide_host_scene_count();
    if index >= count {
        return -1;
    }
    with_app_mut(|app| {
        if app.benchmark_mode {
            app.benchmark_scene_index = index;
            return 0;
        }
        if let Some(router) = app.router.as_mut() {
            router.set_scene(index as usize);
            0
        } else {
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn oxide_host_current_scene() -> u32 {
    APP_STATE
        .get()
        .and_then(|state| state.lock().ok())
        .map(|app| {
            if app.benchmark_mode {
                app.benchmark_scene_index
            } else {
                app.router.as_ref().map(|router| scene_index(router.current)).unwrap_or(0)
            }
        })
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn oxide_host_set_overlay_visible(on: u8) -> ::libc::c_int {
    let desired = on != 0;
    with_app_mut(|app| {
        let prev = app.overlay_visible;
        app.overlay_visible = desired;
        if let Some(router) = app.router.as_mut() {
            if prev != desired {
                router.toggle_overlay();
            }
            app.overlay_dirty = false;
        } else if prev != desired {
            app.overlay_dirty = true;
        }
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn oxide_host_is_overlay_visible() -> u8 {
    app_state().lock().map(|app| if app.overlay_visible { 1 } else { 0 }).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn oxide_host_set_reduce_motion(on: u8) -> ::libc::c_int {
    let desired = on != 0;
    with_app_mut(|app| {
        let prev = app.reduce_motion_on;
        app.reduce_motion_on = desired;
        if let Some(router) = app.router.as_mut() {
            if prev != desired {
                router.set_reduce_motion(desired);
            }
            app.reduce_motion_dirty = false;
        } else if prev != desired {
            app.reduce_motion_dirty = true;
        }
        0
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn oxide_host_is_reduce_motion() -> u8 {
    app_state().lock().map(|app| if app.reduce_motion_on { 1 } else { 0 }).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_pinch(cx: f32, cy: f32, delta: f32) {
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_pinch(cx, cy, delta);
        }
    });
}

#[no_mangle]
pub extern "C" fn oxide_host_emit_double_tap() {
    let _ = with_app_mut(|app| {
        if let Some(router) = app.router.as_mut() {
            router.input_double_tap();
        }
    });
}

fn decode_png_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ()> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().map_err(|_| ())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|_| ())?;
    let bytes = &buf[..info.buffer_size()];
    let out = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            bytes.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect()
        }
        _ => return Err(()),
    };
    Ok((info.width, info.height, out))
}

fn scene_index(kind: test_scenes::SceneKind) -> u32 {
    kind as u32
}

fn gen_checker_rgba(w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let mut data = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for y in 0..h {
        for x in 0..w {
            let dark = ((x / 16) + (y / 16)) % 2 == 0;
            let c = if dark { 0x40 } else { 0xc0 };
            data.push(c);
            data.push(c);
            data.push(c);
            data.push(0xff);
        }
    }
    (w, h, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_touch_tracks_pointer_motion() {
        let mut tracker = PrimaryTouchTracker::default();
        let start = tracker.on_event(42, 0, 10.0, 20.0, 10);
        let start_ptr = start.pointer.expect("pointer on start");
        assert_eq!(start_ptr.buttons, 1);
        let move_ev = tracker.on_event(42, 1, 12.5, 25.0, 20);
        let ptr = move_ev.pointer.expect("pointer on move");
        assert!((ptr.dx - 2.5).abs() < 1e-4);
        assert!((ptr.dy - 5.0).abs() < 1e-4);
        assert_eq!(ptr.buttons, 1);
        let end = tracker.on_event(42, 2, 13.0, 26.0, 30);
        let ptr_end = end.pointer.expect("pointer on end");
        assert_eq!(ptr_end.buttons, 0);
        assert!(!end.double_tap);
    }

    #[test]
    fn primary_touch_detects_double_tap() {
        let mut tracker = PrimaryTouchTracker::default();
        let first_start = tracker.on_event(1, 0, 0.0, 0.0, 0);
        assert!(first_start.pointer.is_some());
        let first_end = tracker.on_event(1, 2, 1.0, 1.0, 150_000_000);
        assert!(!first_end.double_tap);
        let second_start = tracker.on_event(2, 0, 0.0, 0.0, 260_000_000);
        assert!(second_start.pointer.is_some());
        let second_end = tracker.on_event(2, 2, 1.0, 1.0, 340_000_000);
        assert!(second_end.double_tap);
    }
}
#[cfg(target_os = "ios")]
extern "C" {
    fn oxide_host_ios_log(ptr: *const ::libc::c_char, len: usize);
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn ios_env_flag(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn ios_log_enabled() -> bool {
    #[cfg(target_os = "ios")]
    {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| ios_env_flag("OXIDE_RUST_LOG"))
    }
    #[cfg(not(target_os = "ios"))]
    {
        false
    }
}

#[inline(always)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn ios_log(msg: &str) {
    #[cfg(target_os = "ios")]
    unsafe {
        if ios_log_enabled() {
            oxide_host_ios_log(msg.as_ptr() as *const ::libc::c_char, msg.len());
        }
    }

    #[cfg(not(target_os = "ios"))]
    let _ = msg;
}
