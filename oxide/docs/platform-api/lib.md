# platform-api `lib.rs`

## Intention and purpose
- Define Oxide's platform contract and the shared process-global platform registry used by host and app code.
- Provide one source of truth for generic OS services such as redraw, asynchronous HTTP, permissions, sensors, haptics, IME, clipboard, telephony, and media-library access.
- Define camera services around app-visible frame streams so Oxide owns visible preview rendering.

## Relation to the rest of the code
- Host crates install concrete platform implementations through `set_current_platform`.
- App crates such as Nametag resolve services through `current_platform()` and `request_redraw_if_registered()`.
- `SharedPlatform` keeps older boxed-`Platform` APIs working without reintroducing parallel registries.
- Module-specific adapters such as [`secure_storage`](./secure_storage.md) can also expose process-global callback registries when synchronous compatibility shims still need generic OS services before a full app runtime is available.
- Shared normalization/utilities for service adapters live with the corresponding module docs, for example [`telephony`](./telephony.md).

## Entry points list
- `Platform`
  Core trait implemented by each host runtime.
- `set_current_platform(platform)`
  Installs the process-global shared platform.
- `clear_current_platform_for_tests()`
  Clears the registry for deterministic test teardown.
- `current_platform_if_registered() -> Option<Arc<dyn Platform + Send + Sync>>`
  Returns the installed platform without aborting.
- `current_platform() -> Arc<dyn Platform + Send + Sync>`
  Returns the installed platform or aborts with a diagnostic.
- `request_redraw_if_registered() -> bool`
  Issues a redraw through the installed platform when available.
- `SharedPlatform`
  Adapter that forwards boxed `Platform` calls to a shared `Arc`.
- `HttpClient::start(request, callback) -> HttpOperation`
  Starts a bounded streaming request and returns an explicit cancellation handle. `HttpEvent` delivers response metadata, body chunks, and exactly one terminal event.
- `HttpRequest::post(url, body)` and `HttpCredentials`
  Make request bodies and the exceptional same-origin ambient-credential policy explicit; GET and the default request path remain credential-free.

## Logic narrative
- The new registry keeps one `Arc<dyn Platform + Send + Sync>` behind an `RwLock`.
- Hosts install a concrete implementation once per process.
- Callers that still need a boxed trait object wrap the shared instance in `SharedPlatform`, avoiding duplicate host-bridge graphs while preserving existing constructor signatures.
- Camera consumers use `start_stream` for preview pixels; native compositor preview planes are not a public product path.
- Raw `TouchEvent` samples carry `timestamp_ns` so input routing and latency measurement use the OS sample time instead of an out-of-band helper value.
- HTTP operations receive the remaining portion of the caller's absolute budget; request/response sizes, selected response headers, and credentials are explicit, and redirects remain visible so policy stays with the caller.

## Preconditions and postconditions
- A host must install the current platform before app code calls `current_platform()`.
- After installation, redraw requests and service lookups all target the same host object.

## Edge cases and failure modes
- `current_platform()` aborts loudly when called before registration because there is no safe fallback for missing host services.
- `request_redraw_if_registered()` returns `false` instead of aborting so callers can use it opportunistically during early boot/teardown.
- Host-native visible preview transports are benchmark diagnostics only and are not exposed through `platform-api`.
- Unsupported hosts reject HTTP at admission; they never silently run a blocking fallback.

## Concurrency and memory behavior
- Registry state is guarded by an `RwLock`.
- Callers share the platform through cloned `Arc`s; service lifetimes remain owned by the host implementation.
- HTTP callbacks may arrive on host-owned threads. Cancellation is idempotent and a terminal event ends callback ownership.

## Performance notes
- Generic service lookups are one `RwLock` read plus an `Arc` clone.
- This replaces duplicate app-local singleton lookups with one framework-owned registry.
- Camera preview performance is measured through Oxide-owned frame delivery, composition, pacing, and presentation.
- Touch latency attribution starts from `TouchEvent::timestamp_ns`, which is populated by iOS, macOS, and Web hosts.

## Feature flags and cfgs
- Registry behavior is target-agnostic and always enabled.

## Testing and benchmarks
- Registry coverage lives in `docs/platform-api/tests/current_platform_tests.md`; asynchronous HTTP coverage lives in `docs/platform-api/tests/http_tests.md`.

## Examples
```rust
let platform = oxide_platform_api::current_platform();
platform.request_redraw();
```

## Changelog
- 2026-07-11: replaced the blocking HTTP response API with bounded streaming events, remaining-timeout budgets, selected headers, and explicit cancellation.
- 2026-05-31: added `TouchEvent::timestamp_ns` to keep raw input sample time in the platform event contract.
- 2026-05-31: removed the public native-preview API so product preview rendering stays Oxide-owned.
- 2026-03-12: added the current-platform registry helpers and `SharedPlatform` adapter so apps can consume a single Oxide-owned platform instance instead of maintaining duplicate bridge registries.
