//! Oxide macOS host static library
#![allow(clippy::all, clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

#[cfg(target_os = "macos")]
extern "C" {
    fn macos_host_start() -> ::core::ffi::c_int;
}

#[no_mangle]
pub extern "C" fn rust_entry() -> ::libc::c_int {
    #[cfg(target_os = "macos")]
    unsafe {
        macos_host_start() as ::libc::c_int
    }

    #[cfg(not(target_os = "macos"))]
    {
        -1
    }
}

// ===== App state: renderer + scenes router =====

use oxide_input::{touch_phase_from_raw, PrimaryTouchTracker};
use oxide_platform_api as platform_api;
use oxide_renderer_api as gfx_api;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
#[cfg(feature = "host-testing")]
use oxide_telemetry::TelemetryLifecycleState;
use oxide_telemetry::{
    MemoryPressureLevel, TelemetryAction, TelemetryCommandReason, TelemetryHub, TelemetryOperations,
};
use oxide_test_scenes as test_scenes;
use oxide_text as text;
use oxide_timing as timing;
use oxide_ui_core as ui;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

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

#[derive(Default)]
struct AppState {
    renderer: Option<Box<metal::MetalRenderer>>,
    router: Option<test_scenes::Router<MtlUploader>>,
    builder: ui::DrawListBuilder,
    pending_damage_rects: Vec<gfx_api::RectI>,
    prepared_frame: bool,
    touch: PrimaryTouchTracker,
    last_ms: u64,
    inited: bool,
    space_down: bool,
    high_refresh_on: bool,
    reduce_motion_on: bool,
    idle_disabled: bool,
    telemetry: Option<Arc<TelemetryHub>>,
    telemetry_ops: Option<Arc<TelemetryOperations>>,
    frame_dirty: bool,
    settle_frames_remaining: u8,
    idle_skipped_frames: u64,
    submitted_frames: u64,
}

static APP: OnceLock<Mutex<AppState>> = OnceLock::new();

fn app_state() -> &'static Mutex<AppState> {
    APP.get_or_init(|| Mutex::new(AppState::default()))
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn with_app_mut<R>(f: impl FnOnce(&mut AppState) -> R) -> Option<R> {
    APP.get().map(|mtx| {
        let mut guard = lock_or_recover(mtx);
        f(&mut guard)
    })
}

fn callback_value<T: Copy>(cell: &OnceLock<Mutex<Option<T>>>) -> Option<T> {
    cell.get().and_then(|mutex| *lock_or_recover(mutex))
}

const IDLE_SETTLE_FRAMES: u8 = 2;

fn mark_frame_dirty(app: &mut AppState) {
    app.frame_dirty = true;
    app.settle_frames_remaining = IDLE_SETTLE_FRAMES;
}

fn retain_pending_damage_for_retry(
    pending: &mut Vec<gfx_api::RectI>,
    mut damage: Vec<gfx_api::RectI>,
) {
    if damage.is_empty() {
        return;
    }
    if pending.is_empty() {
        *pending = damage;
    } else {
        pending.append(&mut damage);
    }
}

#[no_mangle]
pub extern "C" fn macos_app_init(w: u32, h: u32, scale: f32) -> ::libc::c_int {
    let mut app = lock_or_recover(app_state());
    if app.inited {
        return 0;
    }
    let _platform = oxide_platform_macos::install_current_platform();
    let mut renderer = match metal::MetalRenderer::new_default() {
        Ok(r) => r,
        Err(_) => return -1,
    };
    let _ = renderer.resize(w, h, scale);
    renderer.set_damage_options(true, 0.70, 0.25);
    // Store renderer in a Box so its address remains stable for the uploader.
    let mut boxed = Box::new(renderer);
    let ptr: *mut metal::MetalRenderer = &mut *boxed;
    let uploader = MtlUploader { renderer: ptr };
    let mut router = test_scenes::Router::new(uploader);
    router.damage_set_options(true, 0.70, 0.25);
    let telemetry = Arc::new(TelemetryHub::new());
    router.telemetry_bind(&telemetry);
    app.renderer = Some(boxed);
    app.telemetry = Some(Arc::clone(&telemetry));
    app.telemetry_ops = Some(TelemetryOperations::new(Arc::clone(&telemetry)));
    // Load a default font from bundle (optional)
    if let Some(bytes) = macos_resource_get("fonts/Inter-Regular.ttf") {
        let _fid0 = router.text.fonts.add_font(text::Font::from_bytes(bytes));
    }
    // Load a sample image for the Zoom scene; fallback to procedural checkerboard
    let (zw, zh, zrgba) = if let Some(png_bytes) = macos_resource_get("images/sample.png") {
        if let Ok((w0, h0, data0)) = decode_png_rgba(&png_bytes) {
            (w0, h0, data0)
        } else {
            gen_checker_rgba(256, 256)
        }
    } else {
        gen_checker_rgba(256, 256)
    };
    let tex = unsafe { (*ptr).image_create_rgba8(zw, zh, &zrgba, (zw as usize) * 4) };
    router.set_zoom_image(tex, zw, zh);
    app.router = Some(router);
    app.last_ms = timing::now_ms();
    app.inited = true;
    app.high_refresh_on = true;
    app.reduce_motion_on = false;
    app.idle_disabled = true;
    app.builder.clear();
    mark_frame_dirty(&mut app);
    unsafe {
        macos_set_idle_timer_disabled(1);
    }
    // Register input callbacks to route events to the router
    macos_set_touch_callback(Some(touch_cb));
    macos_set_pointer_callback(Some(pointer_cb));
    macos_set_pinch_callback(Some(pinch_cb));
    macos_set_rotate_callback(Some(rotate_cb));
    macos_set_key_callback(Some(key_cb));
    if let Some(ops) = app.telemetry_ops.as_ref() {
        ops.handle_foreground(timing::now_ms());
    }
    process_telemetry_commands(&mut app);
    0
}

fn process_telemetry_commands(app: &mut AppState) {
    let Some(ops) = app.telemetry_ops.as_ref() else {
        return;
    };
    let commands = ops.drain_commands();
    if commands.is_empty() {
        return;
    }
    for command in commands {
        match command.action {
            TelemetryAction::TrimCaches => {
                if let Some(router) = app.router.as_mut() {
                    router.trim_memory();
                }
            }
            TelemetryAction::FlushMetrics => log_telemetry_metrics(app, command.reason),
            TelemetryAction::PauseSensors
            | TelemetryAction::ResumeSensors
            | TelemetryAction::PauseNetworking
            | TelemetryAction::ResumeNetworking
            | TelemetryAction::RefreshPermissions => {}
        }
    }
}

#[cfg(feature = "host-testing")]
#[derive(Clone, Debug, Default)]
pub struct HostHarnessSnapshot {
    pub inited: bool,
    pub draws: Option<u32>,
    pub instanced: Option<u32>,
    pub last_ms: u64,
    pub router_scene: Option<test_scenes::SceneKind>,
    pub telemetry_lifecycle: Option<TelemetryLifecycleState>,
}

#[cfg(feature = "host-testing")]
pub fn host_harness_reset() {
    let mut app = lock_or_recover(app_state());
    app.router = None;
    app.renderer = None;
    app.telemetry = None;
    app.telemetry_ops = None;
    app.inited = false;
    app.high_refresh_on = true;
    app.reduce_motion_on = false;
    app.idle_disabled = false;
    app.last_ms = 0;
    app.touch = PrimaryTouchTracker::default();
    app.builder.clear();
    app.frame_dirty = true;
    app.settle_frames_remaining = IDLE_SETTLE_FRAMES;
    app.idle_skipped_frames = 0;
    app.submitted_frames = 0;
    platform_api::clear_current_platform_for_tests();
    unsafe {
        macos_set_idle_timer_disabled(0);
    }
    macos_set_touch_callback(None);
    macos_set_pointer_callback(None);
    macos_set_pinch_callback(None);
    macos_set_rotate_callback(None);
    macos_set_key_callback(None);
}

#[cfg(feature = "host-testing")]
pub fn host_harness_snapshot() -> HostHarnessSnapshot {
    let app = lock_or_recover(app_state());
    let mut snap = HostHarnessSnapshot::default();
    snap.inited = app.inited;
    snap.last_ms = app.last_ms;
    snap.router_scene = app.router.as_ref().map(|router| router.current);
    if let Some(renderer) = app.renderer.as_ref() {
        let stats = renderer.last_stats();
        snap.draws = Some(stats.draws);
        snap.instanced = Some(stats.instanced);
    }
    if let Some(telemetry) = app.telemetry.as_ref() {
        let snapshot = telemetry.snapshot();
        snap.telemetry_lifecycle = Some(snapshot.operations.lifecycle);
    }
    snap
}

fn log_telemetry_metrics(app: &AppState, reason: TelemetryCommandReason) {
    if let Some(telemetry) = app.telemetry.as_ref() {
        let snapshot = telemetry.snapshot();
        let network_phase = snapshot
            .network
            .as_ref()
            .map(|metrics| format!("{:?}", metrics.phase))
            .unwrap_or_else(|| "none".to_owned());
        let message = format!(
            "[Telemetry-macOS] reason={:?} lifecycle={:?} health={:?} memory={:?} perms={} reachability={:?} network_phase={}",
            reason,
            snapshot.operations.lifecycle,
            snapshot.health,
            snapshot.memory_pressure,
            snapshot.permissions.len(),
            snapshot.reachability.state,
            network_phase
        );
        eprintln!("{}", message);
    }
}

#[no_mangle]
pub extern "C" fn macos_app_frame(w: u32, h: u32, scale: f32) -> ::libc::c_int {
    macos_app_frame_inner(w, h, scale, core::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn macos_app_should_render() -> u8 {
    let mut app = lock_or_recover(app_state());
    if !app.inited {
        return 1;
    }
    let router_wants_frame =
        app.router.as_ref().map_or(false, test_scenes::Router::wants_next_frame);
    if app.frame_dirty || app.settle_frames_remaining > 0 || router_wants_frame {
        return 1;
    }
    app.idle_skipped_frames = app.idle_skipped_frames.saturating_add(1);
    0
}

#[no_mangle]
pub extern "C" fn macos_app_frame_with_drawable(
    w: u32,
    h: u32,
    scale: f32,
    drawable_ptr: *mut ::libc::c_void,
) -> ::libc::c_int {
    let rc = macos_app_prepare_frame(w, h, scale);
    if rc != 0 {
        return rc;
    }
    macos_app_submit_prepared_frame_with_drawable(drawable_ptr)
}

#[no_mangle]
pub extern "C" fn macos_app_prepare_frame(w: u32, h: u32, scale: f32) -> ::libc::c_int {
    let mut app = lock_or_recover(app_state());
    if !app.inited {
        return -1;
    }
    process_telemetry_commands(&mut app);
    let now = timing::now_ms();
    let dt_ms = (now.saturating_sub(app.last_ms)) as u32;
    app.last_ms = now;
    {
        let renderer = match app.renderer.as_mut().map(|b| b.as_mut()) {
            Some(r) => r,
            None => return -2,
        };
        let _ = renderer.resize(w, h, scale);
    }

    let mut builder = core::mem::take(&mut app.builder);
    builder.clear();
    let vp =
        gfx_api::RectF::new(0.0, 0.0, (w as f32) / scale.max(1.0), (h as f32) / scale.max(1.0));
    let damage_rects = {
        let router = match app.router.as_mut() {
            Some(r) => r,
            None => {
                app.builder = builder;
                return -3;
            }
        };
        router.update(now, dt_ms);
        router.draw(vp, scale, &mut builder);
        router.take_damage()
    };
    {
        let dl = builder.drawlist_mut();
        oxide_ui_core::coalesce_adjacent_draws(dl);
    }
    app.builder = builder;
    retain_pending_damage_for_retry(&mut app.pending_damage_rects, damage_rects);
    app.prepared_frame = true;
    0
}

#[no_mangle]
pub extern "C" fn macos_app_submit_prepared_frame_with_drawable(
    drawable_ptr: *mut ::libc::c_void,
) -> ::libc::c_int {
    let mut app = lock_or_recover(app_state());
    if !app.inited {
        return -1;
    }
    if !app.prepared_frame {
        return -6;
    }
    let mut damage_rects = core::mem::take(&mut app.pending_damage_rects);
    let builder = core::mem::take(&mut app.builder);
    let submit_result = if let Some(renderer) = app.renderer.as_mut().map(|b| b.as_mut()) {
        let present_result = if drawable_ptr.is_null() {
            Ok(())
        } else {
            unsafe { renderer.prepare_present_drawable(drawable_ptr.cast()) }
        };
        if present_result.is_err() {
            Err(-5)
        } else {
            let damage_obj = gfx_api::Damage { rects: core::mem::take(&mut damage_rects) };
            let token = renderer.begin_frame(&gfx_api::FrameTarget, Some(&damage_obj));
            renderer.encode_pass(builder.drawlist());
            if let Err(_) = renderer.submit(token) {
                let _ = renderer.cancel_present_drawable();
                damage_rects = damage_obj.rects;
                Err(-4)
            } else {
                Ok(())
            }
        }
    } else {
        Err(-2)
    };
    app.builder = builder;
    app.prepared_frame = false;
    if let Err(code) = submit_result {
        retain_pending_damage_for_retry(&mut app.pending_damage_rects, damage_rects);
        return code;
    }
    app.submitted_frames = app.submitted_frames.saturating_add(1);
    if app.settle_frames_remaining > 0 {
        app.settle_frames_remaining -= 1;
    }
    app.frame_dirty = false;
    process_telemetry_commands(&mut app);
    0
}

#[no_mangle]
pub extern "C" fn macos_app_cancel_prepared_frame() {
    let mut app = lock_or_recover(app_state());
    app.prepared_frame = false;
    mark_frame_dirty(&mut app);
}

fn macos_app_frame_inner(
    w: u32,
    h: u32,
    scale: f32,
    drawable_ptr: *mut ::libc::c_void,
) -> ::libc::c_int {
    let rc = macos_app_prepare_frame(w, h, scale);
    if rc != 0 {
        return rc;
    }
    macos_app_submit_prepared_frame_with_drawable(drawable_ptr)
}

#[no_mangle]
pub extern "C" fn macos_app_did_become_active() {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_foreground(timing::now_ms());
        }
        process_telemetry_commands(app);
    });
}

#[no_mangle]
pub extern "C" fn macos_app_will_resign_active() {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_background(timing::now_ms());
        }
        process_telemetry_commands(app);
    });
}

#[no_mangle]
pub extern "C" fn macos_app_will_terminate() {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            ops.handle_shutdown(timing::now_ms());
        }
        process_telemetry_commands(app);
    });
}

#[no_mangle]
pub extern "C" fn macos_app_on_memory_pressure(level: u32) {
    with_app_mut(|app| {
        if let Some(ops) = app.telemetry_ops.as_ref() {
            let mapped = match level {
                0 => MemoryPressureLevel::Nominal,
                1 => MemoryPressureLevel::Warning,
                _ => MemoryPressureLevel::Critical,
            };
            ops.handle_memory_pressure(timing::now_ms(), mapped);
        }
        process_telemetry_commands(app);
    });
}

fn decode_png_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ()> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().map_err(|_| ())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|_| ())?;
    let bytes = &buf[..info.buffer_size()];
    // Convert to RGBA8 if needed
    let out = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut v = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in bytes.chunks_exact(3) {
                v.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            v
        }
        png::ColorType::Grayscale => bytes.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            let mut v = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in bytes.chunks_exact(2) {
                v.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            v
        }
        _ => return Err(()),
    };
    Ok((info.width, info.height, out))
}

fn gen_checker_rgba(w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = (((x / 16) + (y / 16)) % 2) as u8;
            let val = if c == 0 { 220 } else { 40 };
            v.extend_from_slice(&[val, val, val, 255]);
        }
    }
    (w, h, v)
}

// ===== Internal input handlers wired at init =====

extern "C" fn touch_cb(id: u64, phase: u32, x: f32, y: f32, ts_ns: u64) {
    if let Some(mut app) = APP.get().map(|m| m.lock().ok()).flatten() {
        let Some(touch_phase) = touch_phase_from_raw(phase) else {
            return;
        };
        let touch_event = platform_api::TouchEvent {
            id: platform_api::TouchId(id),
            phase: touch_phase,
            timestamp_ns: ts_ns,
            x,
            y,
            pressure: None,
            tilt: None,
            device: platform_api::PointerDevice::Mouse,
        };
        let result = app.touch.on_touch(&touch_event, ts_ns);
        if let Some(router) = app.router.as_mut() {
            router.input_touch(&touch_event);
            if let Some(ptr) = result.pointer {
                router.input_pointer(ptr.x, ptr.y, ptr.dx, ptr.dy, ptr.buttons);
            }
            if result.double_tap {
                router.input_double_tap();
            }
        }
        mark_frame_dirty(&mut app);
    }
}

extern "C" fn pointer_cb(x: f32, y: f32, dx: f32, dy: f32, _buttons: u32, _mods: u32, _ts: u64) {
    if let Some(mut app) = APP.get().map(|m| m.lock().ok()).flatten() {
        if let Some(router) = app.router.as_mut() {
            router.input_pointer(x, y, dx, dy, _buttons);
        }
        mark_frame_dirty(&mut app);
    }
}

extern "C" fn pinch_cb(cx: f32, cy: f32, delta: f32, _ts: u64) {
    if let Some(mut app) = APP.get().map(|m| m.lock().ok()).flatten() {
        if let Some(router) = app.router.as_mut() {
            router.input_pinch(cx, cy, delta);
        }
        mark_frame_dirty(&mut app);
    }
}

extern "C" fn rotate_cb(_cx: f32, _cy: f32, _radians: f32, _ts: u64) {
    // Rotation currently unused
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
        if !chars_ptr.is_null() && chars_len > 0 {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(chars_ptr, chars_len))
                .to_string()
        } else {
            String::new()
        }
    };
    let is_up = repeat == 2; // app.m sends 2 on keyUp
    let ch_opt = chars.chars().next();
    if let Some(mut app) = APP.get().map(|m| m.lock().ok()).flatten() {
        // Scene selection and hotkeys
        if let Some(ch) = ch_opt {
            match ch {
                '1' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(0);
                    }
                }
                '2' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(1);
                    }
                }
                '3' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(2);
                    }
                }
                '4' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(3);
                    }
                }
                '5' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(4);
                    }
                }
                '6' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(5);
                    }
                }
                '7' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(6);
                    }
                }
                '8' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(7);
                    }
                }
                '9' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(8);
                    }
                }
                '0' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(9);
                    }
                }
                'q' | 'Q' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(10);
                    }
                }
                'w' | 'W' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(11);
                    }
                }
                'e' | 'E' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(12);
                    }
                }
                't' | 'T' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(13);
                    }
                }
                'y' | 'Y' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(14);
                    }
                }
                'u' | 'U' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(15);
                    }
                }
                'o' | 'O' => {
                    if let Some(router) = app.router.as_mut() {
                        router.key_scene_select(16);
                    }
                }
                ' ' => {
                    if is_up {
                        if let Some(router) = app.router.as_mut() {
                            router.key_space_up();
                        }
                        app.space_down = false;
                    } else if !app.space_down {
                        if let Some(router) = app.router.as_mut() {
                            router.key_space_down();
                        }
                        app.space_down = true;
                    }
                }
                'f' | 'F' => {
                    if !is_up {
                        if let Some(router) = app.router.as_mut() {
                            router.toggle_overlay();
                        }
                    }
                }
                'r' | 'R' => {
                    if !is_up {
                        app.high_refresh_on = !app.high_refresh_on;
                        unsafe {
                            macos_set_high_refresh(if app.high_refresh_on { 1 } else { 0 });
                        }
                    }
                }
                'm' | 'M' => {
                    if !is_up {
                        app.reduce_motion_on = !app.reduce_motion_on;
                        let rm = app.reduce_motion_on;
                        if let Some(router) = app.router.as_mut() {
                            router.set_reduce_motion(rm);
                        }
                    }
                }
                'i' | 'I' => {
                    if !is_up {
                        app.idle_disabled = !app.idle_disabled;
                        unsafe {
                            macos_set_idle_timer_disabled(if app.idle_disabled { 1 } else { 0 });
                        }
                    }
                }
                'z' | 'Z' => {
                    if !is_up {
                        if let Some(router) = app.router.as_mut() {
                            router.key_zoom_reset();
                        }
                    }
                }
                _ => {}
            }
        }
        // Arrow keys via hardware codes (macOS): left=123, right=124, down=125, up=126
        match code {
            123 => {
                if !is_up {
                    if let Some(router) = app.router.as_mut() {
                        router.key_arrow_left();
                    }
                }
            }
            124 => {
                if !is_up {
                    if let Some(router) = app.router.as_mut() {
                        router.key_arrow_right();
                    }
                }
            }
            125 => {
                if !is_up {
                    if let Some(router) = app.router.as_mut() {
                        router.key_arrow_down();
                    }
                }
            }
            126 => {
                if !is_up {
                    if let Some(router) = app.router.as_mut() {
                        router.key_arrow_up();
                    }
                }
            }
            _ => {}
        }
        mark_frame_dirty(&mut app);
    }
}

// ---- Input callbacks (touch/pointer/key) ----

type TouchCb = extern "C" fn(
    id: u64,
    phase: u32, // 0 Start, 1 Move, 2 End, 3 Cancel
    x: f32,
    y: f32,
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
pub extern "C" fn macos_set_touch_callback(cb: Option<TouchCb>) {
    let slot = TOUCH_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(slot) = cb;
}
#[no_mangle]
pub extern "C" fn macos_set_pointer_callback(cb: Option<PointerCb>) {
    let slot = POINTER_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(slot) = cb;
}
#[no_mangle]
pub extern "C" fn macos_set_key_callback(cb: Option<KeyCb>) {
    let slot = KEY_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(slot) = cb;
}

#[no_mangle]
pub extern "C" fn macos_emit_touch(id: u64, phase: u32, x: f32, y: f32, ts_ns: u64) {
    if let Some(cb) = callback_value(&TOUCH_CB) {
        cb(id, phase, x, y, ts_ns);
    }
}
#[no_mangle]
pub extern "C" fn macos_emit_pointer(
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    buttons: u32,
    modifiers: u32,
    ts_ns: u64,
) {
    if let Some(cb) = callback_value(&POINTER_CB) {
        cb(x, y, dx, dy, buttons, modifiers, ts_ns);
    }
}
#[no_mangle]
pub extern "C" fn macos_emit_key(
    code: u32,
    chars_ptr: *const u8,
    chars_len: usize,
    repeat: u8,
    modifiers: u32,
    ts_ns: u64,
) {
    if let Some(cb) = callback_value(&KEY_CB) {
        cb(code, chars_ptr, chars_len, repeat, modifiers, ts_ns);
    }
}

// ---- Text/IME callbacks ----
type TextCommitCb = extern "C" fn(text_ptr: *const u8, text_len: usize);
type TextCompositionCb = extern "C" fn(start: u32, end: u32, text_ptr: *const u8, text_len: usize);
type TextSelectionCb = extern "C" fn(start: u32, end: u32);

static TEXT_COMMIT_CB: std::sync::OnceLock<std::sync::Mutex<Option<TextCommitCb>>> =
    std::sync::OnceLock::new();
static TEXT_COMPOSE_CB: std::sync::OnceLock<std::sync::Mutex<Option<TextCompositionCb>>> =
    std::sync::OnceLock::new();
static TEXT_SELECT_CB: std::sync::OnceLock<std::sync::Mutex<Option<TextSelectionCb>>> =
    std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn macos_set_text_commit_callback(cb: Option<TextCommitCb>) {
    let slot = TEXT_COMMIT_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(slot) = cb;
}
#[no_mangle]
pub extern "C" fn macos_set_text_composition_callback(cb: Option<TextCompositionCb>) {
    let slot = TEXT_COMPOSE_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(slot) = cb;
}
#[no_mangle]
pub extern "C" fn macos_set_text_selection_callback(cb: Option<TextSelectionCb>) {
    let slot = TEXT_SELECT_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(slot) = cb;
}

#[no_mangle]
pub extern "C" fn macos_emit_text_commit(ptr: *const u8, len: usize) {
    if let Some(cb) = callback_value(&TEXT_COMMIT_CB) {
        cb(ptr, len);
    }
}
#[no_mangle]
pub extern "C" fn macos_emit_text_composition(start: u32, end: u32, ptr: *const u8, len: usize) {
    if let Some(cb) = callback_value(&TEXT_COMPOSE_CB) {
        cb(start, end, ptr, len);
    }
}
#[no_mangle]
pub extern "C" fn macos_emit_text_selection(start: u32, end: u32) {
    if let Some(cb) = callback_value(&TEXT_SELECT_CB) {
        cb(start, end);
    }
}

// ---- Resource loading ----
#[cfg(target_os = "macos")]
extern "C" {
    fn macos_resource_read(name_utf8: *const u8, out_len: *mut usize) -> *mut ::libc::c_void;
    fn macos_free(p: *mut ::libc::c_void);
    fn macos_set_high_refresh(enable: u8);
    fn macos_set_idle_timer_disabled(disabled: u8);
}

/// Load a resource file from the app bundle's Resources directory (macOS only).
#[cfg(target_os = "macos")]
pub fn macos_resource_get(name: &str) -> Option<Vec<u8>> {
    use std::ffi::CString;
    let c = CString::new(name).ok()?;
    let mut len: usize = 0;
    let ptr = unsafe { macos_resource_read(c.as_ptr() as *const u8, &mut len as *mut usize) };
    if ptr.is_null() || len == 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    let out = slice.to_vec();
    unsafe {
        macos_free(ptr);
    }
    Some(out)
}

// ---- Trackpad gestures ----
type PinchCb = extern "C" fn(cx: f32, cy: f32, delta: f32, ts_ns: u64);
type RotateCb = extern "C" fn(cx: f32, cy: f32, radians: f32, ts_ns: u64);
static PINCH_CB: std::sync::OnceLock<std::sync::Mutex<Option<PinchCb>>> =
    std::sync::OnceLock::new();
static ROTATE_CB: std::sync::OnceLock<std::sync::Mutex<Option<RotateCb>>> =
    std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn macos_set_pinch_callback(cb: Option<PinchCb>) {
    let s = PINCH_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(s) = cb;
}
#[no_mangle]
pub extern "C" fn macos_set_rotate_callback(cb: Option<RotateCb>) {
    let s = ROTATE_CB.get_or_init(|| std::sync::Mutex::new(None));
    *lock_or_recover(s) = cb;
}

#[no_mangle]
pub extern "C" fn macos_emit_pinch(cx: f32, cy: f32, delta: f32, ts_ns: u64) {
    if let Some(cb) = callback_value(&PINCH_CB) {
        cb(cx, cy, delta, ts_ns);
    }
}
#[no_mangle]
pub extern "C" fn macos_emit_rotate(cx: f32, cy: f32, radians: f32, ts_ns: u64) {
    if let Some(cb) = callback_value(&ROTATE_CB) {
        cb(cx, cy, radians, ts_ns);
    }
}
