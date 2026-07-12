use oxide_platform_api::{HttpClient, HttpEvent, HttpOperation, HttpRequest, PlatformError};

#[derive(Debug, Default)]
pub struct BrowserHttpClient;

#[cfg(not(target_arch = "wasm32"))]
impl HttpClient for BrowserHttpClient
{
   fn start(&self, _request: HttpRequest, _on_event: Box<dyn Fn(HttpEvent) + Send + Sync>) -> Result<Box<dyn HttpOperation + Send + Sync>, PlatformError>
   {
      Err(PlatformError::Unsupported("browser HTTP requires wasm32"))
   }
}

static CLIENT: BrowserHttpClient = BrowserHttpClient;

pub(crate) const fn client() -> &'static BrowserHttpClient
{
   &CLIENT
}

#[cfg(target_arch = "wasm32")]
mod browser
{
   use super::*;
   use oxide_platform_api::{HttpCredentials, HttpMethod, HttpResponse};
   use js_sys::{Reflect, Uint8Array};
   use std::cell::{Cell, RefCell};
   use std::collections::HashMap;
   use std::sync::Arc;
   use wasm_bindgen::{JsCast, JsValue, closure::Closure};
   use wasm_bindgen_futures::{JsFuture, spawn_local};
   use web_sys::{AbortController, Headers, Request, RequestCredentials, RequestInit, RequestRedirect, Response};

   thread_local!
   {
      static NEXT_ID: Cell<u64> = const { Cell::new(1) };
      static OPERATIONS: RefCell<HashMap<u64, OperationState>> = RefCell::new(HashMap::new());
   }

   const MAXIMUM_ACTIVE_OPERATIONS: usize = 128;
   const MAXIMUM_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
   const MAXIMUM_HEADER_COUNT: usize = 64;
   const MAXIMUM_HEADER_BYTES: usize = 32 * 1024;
   const MAXIMUM_URL_BYTES: usize = 16 * 1024;

   struct OperationState
   {
      controller: AbortController,
      timeout_id: i32,
      _timeout: Closure<dyn FnMut()>,
      callback: Arc<dyn Fn(HttpEvent) + Send + Sync>,
      maximum_bytes: usize,
      received_bytes: usize,
   }

   struct BrowserHttpOperation
   {
      id: u64,
   }

   impl HttpOperation for BrowserHttpOperation
   {
      fn cancel(&self)
      {
         cancel(self.id);
      }
   }

   impl Drop for BrowserHttpOperation
   {
      fn drop(&mut self)
      {
         self.cancel();
      }
   }

   impl HttpClient for BrowserHttpClient
   {
      fn start(&self, request: HttpRequest, on_event: Box<dyn Fn(HttpEvent) + Send + Sync>) -> Result<Box<dyn HttpOperation + Send + Sync>, PlatformError>
      {
         validate(&request)?;
         let remaining = request.timeout;
         if remaining.is_zero()
         {
            return Err(PlatformError::Io(String::from("HTTP deadline exceeded")));
         }
         if OPERATIONS.with(|operations| operations.borrow().len() >= MAXIMUM_ACTIVE_OPERATIONS)
         {
            return Err(PlatformError::Busy);
         }
         let window = web_sys::window().ok_or(PlatformError::Unsupported("window is unavailable"))?;
         let controller = AbortController::new().map_err(|value| unknown("AbortController", value))?;
         let browser_request = browser_request(&request, &controller)?;
         let id = next_id()?;
         let timeout = Closure::wrap(Box::new(move || timeout(id)) as Box<dyn FnMut()>);
         let timeout_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
         let timeout_id = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(timeout.as_ref().unchecked_ref(), timeout_ms)
            .map_err(|value| unknown("setTimeout", value))?;
         OPERATIONS.with(|operations|
         {
            operations.borrow_mut().insert(id, OperationState {
               controller,
               timeout_id,
               _timeout: timeout,
               callback: Arc::from(on_event),
               maximum_bytes: request.max_response_bytes,
               received_bytes: 0,
            });
         });
         spawn_local(run(id, window.fetch_with_request(&browser_request), request.response_headers));
         Ok(Box::new(BrowserHttpOperation { id }))
      }
   }

   fn validate(request: &HttpRequest) -> Result<(), PlatformError>
   {
      if request.url.trim().is_empty() || request.max_response_bytes == 0
      {
         return Err(PlatformError::Invalid("HTTP URL or response limit is invalid"));
      }
      if request.method == HttpMethod::Get && !request.body.is_empty()
      {
         return Err(PlatformError::Invalid("GET request body is not allowed"));
      }
      if request.body.len() > MAXIMUM_REQUEST_BODY_BYTES
         || request.headers.len().saturating_add(request.response_headers.len())
            > MAXIMUM_HEADER_COUNT
      {
         return Err(PlatformError::Invalid("HTTP request bounds are exceeded"));
      }
      let header_bytes = request.headers.iter().fold(0_usize, |total, header| {
         total.saturating_add(header.name.len()).saturating_add(header.value.len())
      }).saturating_add(request.response_headers.iter().map(String::len).sum::<usize>());
      if header_bytes > MAXIMUM_HEADER_BYTES
      {
         return Err(PlatformError::Invalid("HTTP header bytes exceed limit"));
      }
      for header in &request.headers
      {
         if !valid_header_name(header.name.as_str())
            || header.value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
            || header.name.eq_ignore_ascii_case("cookie")
            || header.name.eq_ignore_ascii_case("authorization")
            || header.name.eq_ignore_ascii_case("proxy-authorization")
         {
            return Err(PlatformError::Invalid("HTTP request header is invalid"));
         }
      }
      if request.response_headers.iter().any(|name| !valid_header_name(name))
      {
         return Err(PlatformError::Invalid("selected HTTP response header is invalid"));
      }
      Ok(())
   }

   fn valid_header_name(name: &str) -> bool
   {
      !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
   }

   fn browser_request(request: &HttpRequest, controller: &AbortController) -> Result<Request, PlatformError>
   {
      let init = RequestInit::new();
      init.set_method(match request.method { HttpMethod::Get => "GET", HttpMethod::Post => "POST" });
      init.set_redirect(RequestRedirect::Manual);
      init.set_credentials(match request.credentials {
         HttpCredentials::Omit => RequestCredentials::Omit,
         HttpCredentials::SameOrigin => RequestCredentials::SameOrigin,
      });
      init.set_signal(Some(&controller.signal()));
      let body;
      if !request.body.is_empty()
      {
         body = Uint8Array::from(request.body.as_slice());
         init.set_body(&body);
      }
      let headers = Headers::new().map_err(|value| unknown("Headers", value))?;
      for header in &request.headers
      {
         headers.append(header.name.as_str(), header.value.as_str()).map_err(|value| unknown("header", value))?;
      }
      init.set_headers(&headers);
      Request::new_with_str_and_init(request.url.as_str(), &init).map_err(|value| unknown("Request", value))
   }

   async fn run(id: u64, promise: js_sys::Promise, selected_headers: Vec<String>)
   {
      let response = match JsFuture::from(promise).await
      {
         Ok(value) => match value.dyn_into::<Response>()
         {
            Ok(response) => response,
            Err(value) => return finish(id, HttpEvent::Failed(unknown("fetch response", value))),
         },
         Err(value) => return finish(id, HttpEvent::Failed(unknown("fetch", value))),
      };
      let content_length = response.headers().get("content-length").ok().flatten().and_then(|value| value.parse::<u64>().ok());
      if exceeds_declared_limit(id, content_length)
      {
         return finish(id, HttpEvent::Failed(PlatformError::Io(String::from("HTTP response declared length exceeds limit"))));
      }
      let mut header_bytes = 0_usize;
      let mut headers = Vec::with_capacity(selected_headers.len());
      for name in selected_headers
      {
         let Some(value) = response.headers().get(name.as_str()).ok().flatten() else
         {
            continue;
         };
         header_bytes = header_bytes
            .saturating_add(name.len())
            .saturating_add(value.len());
         if headers.len() >= MAXIMUM_HEADER_COUNT || header_bytes > MAXIMUM_HEADER_BYTES
         {
            return finish(id, HttpEvent::Failed(PlatformError::Io(String::from("selected HTTP response headers exceed limit"))));
         }
         headers.push(oxide_platform_api::HttpHeader { name, value });
      }
      let final_url = response.url();
      if final_url.len() > MAXIMUM_URL_BYTES
      {
         return finish(id, HttpEvent::Failed(PlatformError::Io(String::from("HTTP final URL exceeds limit"))));
      }
      emit(id, HttpEvent::Response(HttpResponse {
         final_url,
         status: response.status(),
         content_length,
         headers,
      }));
      let Some(stream) = response.body() else
      {
         return finish(id, HttpEvent::Complete);
      };
      let reader = match stream.get_reader().dyn_into::<web_sys::ReadableStreamDefaultReader>()
      {
         Ok(reader) => reader,
         Err(value) => return finish(id, HttpEvent::Failed(unknown("response stream reader", value.into()))),
      };
      loop
      {
         let chunk = match JsFuture::from(reader.read()).await
         {
            Ok(chunk) => chunk,
            Err(value) => return finish(id, HttpEvent::Failed(unknown("response stream", value))),
         };
         if Reflect::get(&chunk, &JsValue::from_str("done")).ok().and_then(|value| value.as_bool()).unwrap_or(false)
         {
            return finish(id, HttpEvent::Complete);
         }
         let value = match Reflect::get(&chunk, &JsValue::from_str("value"))
         {
            Ok(value) if !value.is_null() && !value.is_undefined() => value,
            _ => return finish(id, HttpEvent::Failed(PlatformError::Io(String::from("HTTP response stream returned no bytes")))),
         };
         let bytes = Uint8Array::new(&value);
         let length = bytes.byte_length() as usize;
         if length == 0
         {
            continue;
         }
         if !admit_chunk(id, length)
         {
            return finish(id, HttpEvent::Failed(PlatformError::Io(String::from("HTTP response exceeds limit"))));
         }
         emit(id, HttpEvent::Body(bytes.to_vec()));
      }
   }

   fn exceeds_declared_limit(id: u64, content_length: Option<u64>) -> bool
   {
      OPERATIONS.with(|operations|
      {
         operations.borrow().get(&id).is_some_and(|state| content_length.is_some_and(|length| length > state.maximum_bytes as u64))
      })
   }

   fn admit_chunk(id: u64, length: usize) -> bool
   {
      OPERATIONS.with(|operations|
      {
         let mut operations = operations.borrow_mut();
         let Some(state) = operations.get_mut(&id) else { return false; };
         if length > state.maximum_bytes.saturating_sub(state.received_bytes)
         {
            state.controller.abort();
            return false;
         }
         state.received_bytes += length;
         true
      })
   }

   fn emit(id: u64, event: HttpEvent)
   {
      let callback = OPERATIONS.with(|operations| operations.borrow().get(&id).map(|state| state.callback.clone()));
      if let Some(callback) = callback
      {
         callback(event);
      }
   }

   fn finish(id: u64, event: HttpEvent)
   {
      let state = OPERATIONS.with(|operations| operations.borrow_mut().remove(&id));
      if let Some(state) = state
      {
         if let Some(window) = web_sys::window()
         {
            window.clear_timeout_with_handle(state.timeout_id);
         }
         (state.callback)(event);
      }
   }

   fn cancel(id: u64)
   {
      let state = OPERATIONS.with(|operations| operations.borrow_mut().remove(&id));
      if let Some(state) = state
      {
         state.controller.abort();
         if let Some(window) = web_sys::window()
         {
            window.clear_timeout_with_handle(state.timeout_id);
         }
         (state.callback)(HttpEvent::Cancelled);
      }
   }

   fn timeout(id: u64)
   {
      let state = OPERATIONS.with(|operations| operations.borrow_mut().remove(&id));
      if let Some(state) = state
      {
         state.controller.abort();
         (state.callback)(HttpEvent::Failed(PlatformError::Io(String::from("HTTP deadline exceeded"))));
      }
   }

   fn next_id() -> Result<u64, PlatformError>
   {
      NEXT_ID.with(|next|
      {
         let id = next.get();
         let successor = id.checked_add(1).ok_or(PlatformError::Busy)?;
         next.set(successor);
         Ok(id)
      })
   }

   fn unknown(context: &'static str, value: JsValue) -> PlatformError
   {
      let detail = value.as_string().unwrap_or_else(|| String::from("JavaScript error"));
      PlatformError::Unknown(format!("{context}: {detail}"))
   }
}
