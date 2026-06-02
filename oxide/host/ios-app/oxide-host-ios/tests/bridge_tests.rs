use oxide_host_ios::{
    oxide_host_app_shutdown, oxide_host_emit_ime_hidden, oxide_host_emit_ime_shown,
    oxide_host_emit_key, oxide_host_emit_perm, oxide_host_emit_pointer,
    oxide_host_emit_push_notify, oxide_host_emit_push_token, oxide_host_emit_text_commit,
    oxide_host_emit_text_composition, oxide_host_emit_text_selection, oxide_host_emit_touch,
    oxide_host_emit_window_resized, oxide_host_is_overlay_visible, oxide_host_is_reduce_motion,
    oxide_host_set_ime_callbacks, oxide_host_set_key_callback, oxide_host_set_overlay_visible,
    oxide_host_set_perm_callback, oxide_host_set_pointer_callback,
    oxide_host_set_push_notify_callback, oxide_host_set_push_token_callback,
    oxide_host_set_reduce_motion, oxide_host_set_text_commit_callback,
    oxide_host_set_text_composition_callback, oxide_host_set_text_selection_callback,
    oxide_host_set_touch_callback, oxide_host_set_window_resized_callback,
};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};

type WindowArgs = (f32, f32, f32, [f32; 4]);

static WINDOW_ARGS: Mutex<Option<WindowArgs>> = Mutex::new(None);
static TEXT_COMMIT: Mutex<Option<String>> = Mutex::new(None);
static TEXT_COMPOSE: Mutex<Option<(u32, u32, String)>> = Mutex::new(None);
static TEXT_SELECT: Mutex<Option<(u32, u32)>> = Mutex::new(None);
static PUSH_TOKEN: Mutex<Option<(u32, String)>> = Mutex::new(None);
static PUSH_NOTIFY: Mutex<Option<String>> = Mutex::new(None);
static IME_SHOWN: Mutex<Option<(f32, f32, f32, f32)>> = Mutex::new(None);
static IME_HIDDEN: AtomicU32 = AtomicU32::new(0);
static PERM_EVENT: Mutex<Option<(u32, u32)>> = Mutex::new(None);
static TOUCH_EVENTS: AtomicU32 = AtomicU32::new(0);
static POINTER_EVENTS: AtomicU32 = AtomicU32::new(0);
static KEY_EVENTS: AtomicU32 = AtomicU32::new(0);
static APP_STATE_LOCK: Mutex<()> = Mutex::new(());

extern "C" fn window_cb(w: f32, h: f32, scale: f32, l: f32, t: f32, r: f32, b: f32) {
    *WINDOW_ARGS.lock().unwrap() = Some((w, h, scale, [l, t, r, b]));
}

extern "C" fn text_commit_cb(ptr: *const u8, len: usize) {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(slice).into_owned();
    *TEXT_COMMIT.lock().unwrap() = Some(text);
}

extern "C" fn text_compose_cb(start: u32, end: u32, ptr: *const u8, len: usize) {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(slice).into_owned();
    *TEXT_COMPOSE.lock().unwrap() = Some((start, end, text));
}

extern "C" fn text_select_cb(start: u32, end: u32) {
    *TEXT_SELECT.lock().unwrap() = Some((start, end));
}

extern "C" fn push_token_cb(provider: u32, ptr: *const u8, len: usize) {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(slice).into_owned();
    *PUSH_TOKEN.lock().unwrap() = Some((provider, text));
}

extern "C" fn push_notify_cb(ptr: *const u8, len: usize) {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(slice).into_owned();
    *PUSH_NOTIFY.lock().unwrap() = Some(text);
}

extern "C" fn ime_shown_cb(x: f32, y: f32, w: f32, h: f32) {
    *IME_SHOWN.lock().unwrap() = Some((x, y, w, h));
}

extern "C" fn ime_hidden_cb() {
    IME_HIDDEN.fetch_add(1, Ordering::SeqCst);
}

extern "C" fn perm_cb(domain: u32, status: u32) {
    *PERM_EVENT.lock().unwrap() = Some((domain, status));
}

extern "C" fn touch_cb(
    _id: u64,
    _phase: u32,
    _x: f32,
    _y: f32,
    _pressure: f32,
    _has_pressure: u8,
    _tilt_alt: f32,
    _tilt_azi: f32,
    _has_tilt: u8,
    _device: u32,
    _timestamp_ns: u64,
) {
    TOUCH_EVENTS.fetch_add(1, Ordering::SeqCst);
}

extern "C" fn pointer_cb(
    _x: f32,
    _y: f32,
    _dx: f32,
    _dy: f32,
    _buttons: u32,
    _modifiers: u32,
    _timestamp_ns: u64,
) {
    POINTER_EVENTS.fetch_add(1, Ordering::SeqCst);
}

extern "C" fn key_cb(
    _code: u32,
    _chars_ptr: *const u8,
    _chars_len: usize,
    _repeat: u8,
    _modifiers: u32,
    _timestamp_ns: u64,
) {
    KEY_EVENTS.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn window_resize_callback_invoked() {
    let _guard = APP_STATE_LOCK.lock().unwrap();
    oxide_host_set_window_resized_callback(Some(window_cb));
    oxide_host_emit_window_resized(100.0, 200.0, 2.0, 1.0, 2.0, 3.0, 4.0);
    let args = WINDOW_ARGS.lock().unwrap().take().expect("window cb fired");
    assert_eq!(args.0, 100.0);
    assert_eq!(args.1, 200.0);
    assert_eq!(args.2, 2.0);
    assert_eq!(args.3, [1.0, 2.0, 3.0, 4.0]);
    oxide_host_set_window_resized_callback(None);
}

#[test]
fn text_callbacks_forward_payload() {
    let _guard = APP_STATE_LOCK.lock().unwrap();
    oxide_host_set_text_commit_callback(Some(text_commit_cb));
    oxide_host_emit_text_commit(b"hello".as_ptr(), 5);
    assert_eq!(TEXT_COMMIT.lock().unwrap().take(), Some("hello".into()));
    oxide_host_set_text_commit_callback(None);

    oxide_host_set_text_composition_callback(Some(text_compose_cb));
    oxide_host_emit_text_composition(1, 4, b"abcd".as_ptr(), 4);
    assert_eq!(TEXT_COMPOSE.lock().unwrap().take(), Some((1, 4, "abcd".into())));
    oxide_host_set_text_composition_callback(None);

    oxide_host_set_text_selection_callback(Some(text_select_cb));
    oxide_host_emit_text_selection(3, 7);
    assert_eq!(TEXT_SELECT.lock().unwrap().take(), Some((3, 7)));
    oxide_host_set_text_selection_callback(None);
}

#[test]
fn push_callbacks_capture_data() {
    let _guard = APP_STATE_LOCK.lock().unwrap();
    oxide_host_set_push_token_callback(Some(push_token_cb));
    let token = b"abcdef";
    oxide_host_emit_push_token(9, token.as_ptr(), token.len());
    assert_eq!(PUSH_TOKEN.lock().unwrap().take(), Some((9, "abcdef".into())));
    oxide_host_set_push_token_callback(None);

    oxide_host_set_push_notify_callback(Some(push_notify_cb));
    oxide_host_emit_push_notify(b"{\"aps\":1}".as_ptr(), 9);
    assert_eq!(PUSH_NOTIFY.lock().unwrap().take(), Some("{\"aps\":1}".into()));
    oxide_host_set_push_notify_callback(None);
}

#[test]
fn permission_and_input_callbacks_forward_events() {
    let _guard = APP_STATE_LOCK.lock().unwrap();

    oxide_host_set_perm_callback(Some(perm_cb));
    oxide_host_emit_perm(4, 2);
    assert_eq!(PERM_EVENT.lock().unwrap().take(), Some((4, 2)));
    oxide_host_set_perm_callback(None);

    TOUCH_EVENTS.store(0, Ordering::SeqCst);
    POINTER_EVENTS.store(0, Ordering::SeqCst);
    KEY_EVENTS.store(0, Ordering::SeqCst);
    oxide_host_set_touch_callback(Some(touch_cb));
    oxide_host_set_pointer_callback(Some(pointer_cb));
    oxide_host_set_key_callback(Some(key_cb));
    oxide_host_emit_touch(10, 0, 1.0, 2.0, 0.5, 1, 0.0, 0.0, 0, 0, 100);
    oxide_host_emit_pointer(1.0, 2.0, 3.0, 4.0, 1, 2, 101);
    oxide_host_emit_key(33, b"x".as_ptr(), 1, 0, 2, 102);
    assert_eq!(TOUCH_EVENTS.load(Ordering::SeqCst), 1);
    assert_eq!(POINTER_EVENTS.load(Ordering::SeqCst), 1);
    assert_eq!(KEY_EVENTS.load(Ordering::SeqCst), 1);
    oxide_host_set_touch_callback(None);
    oxide_host_set_pointer_callback(None);
    oxide_host_set_key_callback(None);
}

#[test]
fn fallback_emitters_accept_null_empty_payloads() {
    let _guard = APP_STATE_LOCK.lock().unwrap();

    oxide_host_set_push_token_callback(None);
    oxide_host_set_push_notify_callback(None);
    oxide_host_set_text_commit_callback(None);
    oxide_host_set_text_composition_callback(None);
    oxide_host_set_key_callback(None);

    oxide_host_emit_push_token(0, core::ptr::null(), 0);
    oxide_host_emit_push_notify(core::ptr::null(), 0);
    oxide_host_emit_text_commit(core::ptr::null(), 0);
    oxide_host_emit_text_composition(0, 0, core::ptr::null(), 0);
    oxide_host_emit_key(0, core::ptr::null(), 0, 0, 0, 0);
}

#[test]
fn ime_callbacks_record_events() {
    let _guard = APP_STATE_LOCK.lock().unwrap();
    oxide_host_set_ime_callbacks(Some(ime_shown_cb), Some(ime_hidden_cb));
    oxide_host_emit_ime_shown(10.0, 20.0, 30.0, 40.0);
    assert_eq!(IME_SHOWN.lock().unwrap().take(), Some((10.0, 20.0, 30.0, 40.0)));
    oxide_host_emit_ime_hidden();
    assert_eq!(IME_HIDDEN.swap(0, Ordering::SeqCst), 1);
    oxide_host_set_ime_callbacks(None, None);
}

#[test]
fn overlay_toggle_succeeds_without_router() {
    let _guard = APP_STATE_LOCK.lock().unwrap();
    oxide_host_app_shutdown();
    assert_eq!(oxide_host_is_overlay_visible(), 1);
    assert_eq!(oxide_host_set_overlay_visible(0), 0);
    assert_eq!(oxide_host_is_overlay_visible(), 0);
    assert_eq!(oxide_host_set_overlay_visible(0), 0);
    assert_eq!(oxide_host_is_overlay_visible(), 0);
    assert_eq!(oxide_host_set_overlay_visible(1), 0);
    assert_eq!(oxide_host_is_overlay_visible(), 1);
    oxide_host_app_shutdown();
}

#[test]
fn reduce_motion_toggle_succeeds_without_router() {
    let _guard = APP_STATE_LOCK.lock().unwrap();
    oxide_host_app_shutdown();
    assert_eq!(oxide_host_is_reduce_motion(), 0);
    assert_eq!(oxide_host_set_reduce_motion(1), 0);
    assert_eq!(oxide_host_is_reduce_motion(), 1);
    assert_eq!(oxide_host_set_reduce_motion(1), 0);
    assert_eq!(oxide_host_is_reduce_motion(), 1);
    assert_eq!(oxide_host_set_reduce_motion(0), 0);
    assert_eq!(oxide_host_is_reduce_motion(), 0);
    oxide_host_app_shutdown();
}
