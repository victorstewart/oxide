use oxide_host_ios::{
    oxide_host_app_shutdown, oxide_host_emit_ime_hidden, oxide_host_emit_ime_shown,
    oxide_host_emit_push_notify, oxide_host_emit_push_token, oxide_host_emit_text_commit,
    oxide_host_emit_text_composition, oxide_host_emit_text_selection,
    oxide_host_emit_window_resized, oxide_host_is_overlay_visible, oxide_host_is_reduce_motion,
    oxide_host_set_ime_callbacks, oxide_host_set_overlay_visible,
    oxide_host_set_push_notify_callback, oxide_host_set_push_token_callback,
    oxide_host_set_reduce_motion, oxide_host_set_text_commit_callback,
    oxide_host_set_text_composition_callback, oxide_host_set_text_selection_callback,
    oxide_host_set_window_resized_callback,
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
