# platform-web::lib

## Intention and purpose

`oxide-platform-web` adapts Oxide's platform API to browser WebAssembly hosts. It provides a real browser bridge for services that fit Oxide's current synchronous platform contracts, and it makes OS-only surfaces explicit unsupported paths rather than pretending they exist.

## Relation to the rest of the code

The crate depends on `oxide-platform-api` and is consumed by `oxide-host-web`. The host installs `WebPlatform` into `oxide_platform_api::set_current_platform`, then the rest of Oxide can use the same platform trait calls as iOS and macOS hosts.

Call flow:

- `oxide_host_web::start_oxide`
- `oxide_platform_web::install_current_platform`
- `oxide_platform_api::set_current_platform`
- Oxide UI or app code calls `current_platform`
- `oxide_platform_web::WebPlatform` dispatches browser-backed or unsupported service behavior

## Entry points list

- `oxide_platform_web::WebPlatform::new() -> Self`: constructs the zero-sized browser platform adapter.
- `oxide_platform_web::platform() -> WebPlatform`: returns a standalone platform value.
- `oxide_platform_web::install_current_platform() -> Arc<WebPlatform>`: installs `WebPlatform` and its clipboard provider in global Oxide registries.
- `impl oxide_platform_api::Platform for WebPlatform`: implements redraw signaling, refresh/idling toggles, URL opening, clipboard, IME events, device caps, haptics, service accessors, and browser capability reporting.
- `impl oxide_platform_api::clipboard::ClipboardProvider for WebPlatform`: lets registry-based clipboard callers use the browser adapter.
- `impl oxide_platform_api::LocationService for WebLocation`: maps browser geolocation watch/current-position callbacks into Oxide location events on wasm and explicit unsupported fallbacks on native test builds.
- `impl oxide_platform_api::network_status::NetworkStatusService for WebNetworkStatus`: reports `navigator.onLine` and installs `online`/`offline` window listeners for subscriptions on wasm.
- `impl oxide_platform_api::web_view::WebViewService for WebViewService`: creates hidden same-origin iframes on wasm and executes scripts through the iframe window when same-origin access is available.
- `oxide_platform_web::clipboard_read_string_async() -> Result<Option<String>, PlatformError>`: reads the browser clipboard through the async permission-gated Clipboard API where available.
- `oxide_platform_web::clipboard_write_string_async(value: &str) -> Result<(), PlatformError>`: writes the browser clipboard through the async Clipboard API and updates the synchronous cache.
- `oxide_platform_web::start_browser_media_stream(video: bool, audio: bool) -> Result<BrowserMediaStream, PlatformError>`: wasm-only async adapter around `navigator.mediaDevices.getUserMedia`.
- `oxide_platform_web::refresh_rate_hz_from_frame_deltas(deltas_ms: &[f64]) -> u32`: estimates display refresh from RAF sample deltas.
- `oxide_platform_web::hex_encode(bytes: &[u8]) -> String`: encodes secure-storage bytes for localStorage.
- `oxide_platform_web::hex_decode(input: &str) -> Result<Vec<u8>, PlatformError>`: decodes secure-storage bytes and validates malformed data.

## Logic narrative

`WebPlatform` deliberately stores no DOM handles so it can satisfy Oxide's `Platform: Send + Sync` bound. Browser objects are fetched on demand inside wasm-only helper functions, while non-`Send` browser handles and closures live in thread-local registries behind integer handles. `request_redraw`, `ime_show`, and `ime_hide` dispatch custom window events for the host runtime. Clipboard writes cache the last app-written string synchronously and best-effort forward to `navigator.clipboard.writeText`; async helper functions expose the real browser Clipboard promise path for callers that can await. Secure storage uses localStorage with a fixed key prefix and hex-encoded bytes.

Browser network status maps to `navigator.onLine`; subscriptions install one `online` and one `offline` window listener and fan out the updated status. Browser geolocation uses `watchPosition` for `start`, `getCurrentPosition` for `request_once`, a bounded 256-reading history, cached last reading, and permission status updates driven by browser callbacks. Hidden web views are same-origin iframes; cross-origin URLs are rejected because browser security prevents script execution through the current Oxide trait. `set_idle_timer_disabled(true)` requests a screen wake lock where supported and releases it when disabled. Display refresh starts at 60 Hz and is refined from a short requestAnimationFrame delta probe.

Services that do not fit the browser or the current synchronous API return `PlatformError::Unsupported`, `PermissionStatus::Denied`, empty histories, or no-op subscriptions. This includes the synchronous `CameraManager` stream/recording entry points, Bluetooth GATT, motion, push notifications, raw TCP/UDP/QUIC, cross-origin WebViews, telephony, contacts, and media library access. Camera/microphone capture is available through the wasm-only async `start_browser_media_stream` adapter because `getUserMedia` cannot be represented as a synchronous `CameraManager::start_stream` return.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Browser-backed helpers require a wasm32 target and a live browser `window`. Non-wasm builds return deterministic fallbacks so native cargo tests can run. Secure storage keys are prefixed with `oxide.secure.` and values are always hex strings. WebView handles are process-local ids and are removed from the registry on `close` or drop. The crate forbids unsafe code.

## Edge cases and failure modes

If browser globals are missing, URL open, localStorage, geolocation, and WebView operations return `Unsupported` or `Unknown` errors. Clipboard reads return only the last app-written value because the browser Clipboard API is asynchronous and user-permission gated, while Oxide's current read trait is synchronous. Browser location permission starts as not determined on wasm and moves to authorized or denied only after browser callbacks. Unsupported service methods do not panic.

## Concurrency and memory behavior

The platform object is zero-sized and can be shared through `Arc`. The clipboard cache uses `RwLock<Option<String>>`. Wasm-only service state uses thread-local registries because DOM handles and `Closure` values are single-threaded JavaScript objects. Futures returned by secure storage and WebView script execution capture only already-computed results so they satisfy the platform API's `Send` future bounds.

## Performance notes

Platform methods are not frame-loop hot paths except `request_redraw`, device caps reads, and time reads. `request_redraw` dispatches a single custom browser event. Network listeners are installed once. Location history is bounded. Secure storage hex encoding is linear in byte length and intended for small secrets, not large blobs.

## Feature flags and cfgs

Browser integrations compile only under `target_arch = "wasm32"`. Non-wasm builds keep tests deterministic by returning fallback values and explicit unsupported errors.

## Testing and benchmarks

`oxide/crates/platform-web/tests/lib_tests.rs` covers secure-storage hex encoding, refresh-rate estimation, device caps fallbacks, unsupported OS service behavior, browser-only native fallback boundaries, clipboard caching, and haptics no-op acceptance. The static wasm host page also calls `platform_smoke_report`, which exercises browser capability reporting, network subscription installation, and same-origin iframe WebView create/close in Chromium. `oxide-perf-runner` includes `cpu.bridge.web_backend_surface` so the native fallback surface remains represented in the persisted workspace performance contract.

## Examples

```rust
pub fn install_platform()
{
   let _platform = oxide_platform_web::install_current_platform();
}
```

## Changelog

- Added native perf-runner bridge coverage for the web backend surface.
- Added wake lock, RAF refresh-rate probing, async clipboard helpers, and async browser media stream adapter support.
- Added browser network subscriptions, geolocation callbacks, same-origin iframe WebViews, and the host smoke hook.
