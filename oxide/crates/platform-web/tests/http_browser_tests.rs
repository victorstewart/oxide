#![cfg(target_arch = "wasm32")]

use oxide_platform_api::{HttpClient, HttpEvent, HttpRequest};
use oxide_platform_web::BrowserHttpClient;
use std::sync::{Arc, Mutex};
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn tick()
{
   let promise = js_sys::Promise::new(&mut |resolve, _reject|
   {
      let callback = Closure::once_into_js(move ||
      {
         let _ = resolve.call0(&wasm_bindgen::JsValue::UNDEFINED);
      });
      web_sys::window()
         .expect("window")
         .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0)
         .expect("setTimeout");
   });
   let _ = JsFuture::from(promise).await;
}

async fn wait_terminal(events: &Arc<Mutex<Vec<HttpEvent>>>)
{
   for _ in 0..100
   {
      if events.lock().expect("events").iter().any(HttpEvent::terminal)
      {
         return;
      }
      tick().await;
   }
   panic!("browser HTTP operation did not terminate");
}

#[wasm_bindgen_test(async)]
async fn fetch_streams_response_and_delivers_exactly_one_terminal_event()
{
   let events = Arc::new(Mutex::new(Vec::new()));
   let callback_events = events.clone();
   let _operation = BrowserHttpClient.start(
      HttpRequest::get("data:text/plain,oxide")
         .with_max_response_bytes(32)
         .select_response_header("content-type"),
      Box::new(move |event| callback_events.lock().expect("events").push(event)),
   ).expect("start browser fetch");
   wait_terminal(&events).await;
   let events = events.lock().expect("events");
   assert!(matches!(events.first(), Some(HttpEvent::Response(response))
      if response.status == 200
         && response.headers.iter().any(|header| {
            header.name == "content-type" && header.value.starts_with("text/plain")
         })));
   assert_eq!(events.iter().filter(|event| event.terminal()).count(), 1);
   assert!(matches!(events.last(), Some(HttpEvent::Complete)));
   assert_eq!(events.iter().filter_map(|event| match event {
      HttpEvent::Body(bytes) => Some(bytes.as_slice()),
      _ => None,
   }).flatten().copied().collect::<Vec<_>>(), b"oxide");
}

#[wasm_bindgen_test(async)]
async fn cancel_aborts_fetch_and_ignores_every_late_browser_callback()
{
   let events = Arc::new(Mutex::new(Vec::new()));
   let callback_events = events.clone();
   let operation = BrowserHttpClient.start(
      HttpRequest::get("data:text/plain,late"),
      Box::new(move |event| callback_events.lock().expect("events").push(event)),
   ).expect("start browser fetch");
   operation.cancel();
   for _ in 0..5
   {
      tick().await;
   }
   let events = events.lock().expect("events");
   assert_eq!(events.as_slice(), &[HttpEvent::Cancelled]);
}

#[wasm_bindgen_test(async)]
async fn streamed_response_cap_fails_closed()
{
   let events = Arc::new(Mutex::new(Vec::new()));
   let callback_events = events.clone();
   let _operation = BrowserHttpClient.start(
      HttpRequest::get("data:text/plain,too-large").with_max_response_bytes(3),
      Box::new(move |event| callback_events.lock().expect("events").push(event)),
   ).expect("start browser fetch");
   wait_terminal(&events).await;
   let events = events.lock().expect("events");
   assert_eq!(events.iter().filter(|event| event.terminal()).count(), 1);
   assert!(!events.iter().any(|event| matches!(event, HttpEvent::Body(_))));
   assert!(matches!(events.last(), Some(HttpEvent::Failed(_))));
}

#[wasm_bindgen_test(async)]
async fn streamed_response_exact_cap_succeeds()
{
   let events = Arc::new(Mutex::new(Vec::new()));
   let callback_events = events.clone();
   let _operation = BrowserHttpClient.start(
      HttpRequest::get("data:text/plain,abc").with_max_response_bytes(3),
      Box::new(move |event| callback_events.lock().expect("events").push(event)),
   ).expect("start browser fetch");
   wait_terminal(&events).await;
   let events = events.lock().expect("events");
   assert_eq!(events.iter().filter(|event| event.terminal()).count(), 1);
   assert_eq!(events.iter().filter_map(|event| match event {
      HttpEvent::Body(bytes) => Some(bytes.as_slice()),
      _ => None,
   }).flatten().copied().collect::<Vec<_>>(), b"abc");
   assert!(matches!(events.last(), Some(HttpEvent::Complete)));
}

#[wasm_bindgen_test(async)]
async fn selected_response_header_values_share_the_fixed_header_byte_budget()
{
   let events = Arc::new(Mutex::new(Vec::new()));
   let callback_events = events.clone();
   let url = format!("data:text/plain;x={},ok", "a".repeat(33 * 1024));
   let _operation = BrowserHttpClient.start(
      HttpRequest::get(url)
         .with_max_response_bytes(8)
         .select_response_header("content-type"),
      Box::new(move |event| callback_events.lock().expect("events").push(event)),
   ).expect("start browser fetch");
   wait_terminal(&events).await;
   let events = events.lock().expect("events");
   assert_eq!(events.iter().filter(|event| event.terminal()).count(), 1);
   assert!(matches!(events.last(), Some(HttpEvent::Failed(_))));
}
