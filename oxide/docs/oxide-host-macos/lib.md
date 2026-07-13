# oxide-host-macos `lib.rs`

## Intention and purpose
- Own the Rust side of the macOS AppKit host: renderer setup, scene routing, input callback registration, lifecycle telemetry, and platform installation.

## Relation to the rest of the code
- AppKit calls exported `macos_app_*` entry points from `src/macos/app.m`.
- The host installs `oxide-platform-macos` into `oxide-platform-api` so author-facing platform services can be resolved process-wide.
- The host uses `oxide-input` for shared primary touch-to-pointer behavior and `oxide-renderer-metal` for frame rendering.
- Native permission, network-status, location, motion-availability, media-library, push, WebView, and device-capability calls are implemented in the AppKit shim and consumed through `oxide-platform-macos`; HTTP and secure storage are compiled from shared Apple native bridges; raw TCP/UDP networking and TCP keepalive setup are provided by the shared Apple socket backend reached through the installed macOS platform.
- The host guards UserNotifications access for non-bundled test processes so notification status/register/clear paths degrade to no-op or not-determined results instead of aborting.

## Entry points list
- `rust_entry() -> libc::c_int`
  Starts the native macOS host.
- `macos_app_init(w, h, scale) -> libc::c_int`
  Initializes renderer, test-scene router, telemetry, callbacks, and process-global platform state.
- `macos_app_frame(...)` and `macos_app_frame_with_drawable(...)`
  Drive frame rendering for headless and drawable-backed paths.
- `host_harness_reset()` and `host_harness_snapshot()`
  Test-only state reset and inspection helpers.
- `oxide_platform_api::current_platform_if_registered().secure_storage()`
  Reached indirectly by the host harness to verify the installed macOS platform uses the shared live Keychain-backed secure-storage ABI.
- `oxide_platform_api::current_platform_if_registered().http()`
  Reached indirectly by the host harness to verify the installed macOS platform uses the shared native `NSURLSession` HTTP ABI and shared Apple response wrapper.
- `oxide_platform_api::current_platform_if_registered().networking()`
  Reached indirectly by the host harness to verify the installed macOS platform uses the shared Apple raw TCP/UDP socket backend.
- `oxide_platform_api::current_platform_if_registered().web_view_service()`
  Reached indirectly by the WebView harness to verify the installed macOS platform creates a live hidden `WKWebView`, receives load completion, and executes JavaScript.

## Logic narrative
- Initialization creates a stable boxed Metal renderer and router, then registers raw input callbacks.
- Touch callbacks decode raw phases through `oxide-input`, feed raw touch events into the router, and synthesize primary pointer/double-tap events from the shared tracker.
- Host lifecycle and callback registries use poison-recovering mutex locks because these entry points are called from native AppKit/FFI boundaries where a panic would be harder to contain than recovering the last known state.
- Input, text, pinch, and rotate emitters copy the registered function pointer while holding the callback mutex, then invoke the function pointer after the lock is released by value-copying the `extern "C" fn` option.
- Test reset clears the process-global platform registry so harness tests can prove `macos_app_init` installs it.
- The secure-storage host test initializes the macOS platform through the same host path, then saves, loads, deletes, and confirms deletion for a unique Keychain item through the shared Apple Rust wrapper and shared native Keychain bridge.
- The HTTP host test starts a loopback TCP server, then fetches it through `MacPlatform::http()` so the request passes through the shared native `NSURLSession` bridge rather than a test-local HTTP stub.
- The raw networking host tests start loopback TCP and UDP echo servers, then connect/send through `MacPlatform::networking()` so platform installation and shared Apple socket wiring are verified together. The same harness verifies TCP keepalive through a loopback echo path and verifies raw TCP TLS, TCP Fast Open, and QUIC fail explicitly as unsupported through the installed platform.
- The WebView host test is a custom main-thread harness because `WKWebView` creation, location permission status, and script evaluation must run with the process main run loop available rather than a libtest worker thread.
- The main-thread harness also verifies safe host controls, paths, time/device caps, capability coherence, telephony no-provider, haptics dispatch, Bluetooth cache shape, native permission-status snapshots, network-status current/subscription snapshots, no-prompt location/motion/camera/push/media-library behavior, and hidden-WebView lifecycle/script behavior through the installed platform without triggering permission prompts. It includes opt-in `OXIDE_MACOS_LIVE_LOCATION=1`, `OXIDE_MACOS_LIVE_CAMERA=1`, and `OXIDE_MACOS_LIVE_MEDIA=1` paths for pre-authorized Location Services, camera hardware, and Photos libraries.

## Preconditions and postconditions
- Native FFI functions for callback registration and host lifecycle must be linked by the AppKit host.
- After successful initialization, renderer, router, telemetry, input callbacks, and current platform are installed.

## Edge cases and failure modes
- Renderer creation failure returns `-1`.
- Headless frame calls preserve the non-drawable path used by tests.
- Drawable-backed frame submission preserves pending damage when a prepared frame is canceled or fails before presentation, so a timeout from late `nextDrawable` acquisition retries the dirty region on the next frame instead of losing the redraw.
- Poisoned lifecycle or callback mutexes are recovered instead of panicking, preserving native host liveness after an earlier Rust-side unwind.
- Missing callback registrations simply drop the emitted host event.
- Secure-storage tests use a unique key and delete before and after the round trip so stale Keychain data does not affect results.
- HTTP loopback tests use `127.0.0.1` and a single in-process response so they do not depend on external network access.
- TCP/UDP loopback tests use only local sockets, timeout waiting for callback delivery, and still exercise the same background reader threads used by live networking.
- Unsupported raw transport tests verify TLS/QUIC/TFO rejection without depending on an external listener or network route.
- WebView verification loads temporary local HTML files into concurrent hidden views and evaluates JavaScript result/error cases, avoiding external network dependency while exercising real WebKit navigation, callback routing, script, and teardown paths.
- Permission-status verification covers every Oxide permission domain as a no-prompt status query; notification status is expected to return not-determined in non-bundled host-test processes.
- Network-status verification checks the immediate status snapshot and subscriber callback shape without asserting a particular machine connectivity state.
- Basic service verification checks redraw, refresh toggle, idle-timer toggle, IME no-ops, simulation reporting, standard directories, monotonic time, device caps, capability bit relationships, telephony no-provider, haptics dispatch, and Bluetooth cached-entry shape without starting Bluetooth hardware operations.
- No-prompt service verification covers location last/accuracy, motion no-provider start, macOS native camera preview rejection, macOS camera unsupported controls, inactive camera photo/recording missing-session mapping, unauthorized camera startup permission mapping, non-bundled push register/token/badge/clear behavior, PhotoKit media-library query/missing-asset request mapping, and authorized image-thumbnail extraction when an image asset is already available. The opt-in live location path covers CoreLocation start, request-once, update callback delivery, cached last reading, history, and stop.

## Concurrency and memory behavior
- App state is guarded by one process-global mutex and recovered on poison at every host entry point.
- The visible Metal renderer owns three completion-protected frame slots; it skips/coalesces when all remain in flight rather than waiting or reusing a busy slot.
- Callback slots are `OnceLock<Mutex<Option<extern "C" fn(...)>>>` values; emitters copy function pointers out of the lock before dispatch.
- Callback tests serialize access to global callback slots so parallel test execution cannot race the smoke harness.
- The custom WebView harness runs as process `main`, pumps the CoreFoundation run loop while waiting for WebKit callbacks, and removes its temporary HTML document before teardown.

## Testing and benchmarks
- Covered by `cargo test -p oxide-host-macos --features host-testing --tests --locked`.
- `tests/headless_harness.rs` verifies host initialization, frame routing, platform installation, live Keychain secure-storage round-trip/delete, native loopback HTTP GET, installed-platform TCP/UDP loopback networking, installed-platform TCP keepalive, installed-platform unsupported transport rejection, and callback fanout for touch, pointer, key, text, pinch, and rotate paths.
- `tests/web_view_harness.rs` verifies installed-platform basic services, permission-status snapshots, network-status snapshots/subscription callbacks, no-prompt location/motion/camera/push/media-library behavior, opt-in live location updates on pre-authorized Location Services, opt-in live camera frame/photo/recording behavior on pre-authorized hardware, opt-in live media image/video extraction on pre-authorized Photos libraries, and WebView lifecycle/script behavior through live hidden `WKWebView` instances.
- `tests/metal_drawable_lifetime_tests.rs` statically verifies late drawable acquisition, timeout-capable `nextDrawable`, cancellation, and pending-damage retention after skipped prepared frames.

## Changelog
- 2026-07-13: selected the three-slot visible Metal frame-resource mode while leaving eight-slot depth explicit for offscreen/perf construction.
- 2026-06-01: retained prepared-frame damage across macOS drawable timeout or submit failure so dirty regions retry after pressure.
- 2026-05-19: moved the macOS secure-storage host ABI to the shared Apple native Keychain bridge.
- 2026-05-19: moved the macOS HTTP host ABI to the shared Apple native URLSession bridge.
- 2026-05-19: added installed-platform TCP keepalive loopback coverage through the shared Apple socket backend.
- 2026-05-19: added an opt-in live macOS location harness for pre-authorized CoreLocation update validation.
- 2026-05-19: expanded installed-platform WebView lifecycle/script coverage through concurrent hidden `WKWebView` instances.
- 2026-05-19: added authorized media-library thumbnail extraction plus an opt-in live media image/video extraction harness.
- 2026-05-19: added no-prompt camera missing-session/startup permission-error coverage and an opt-in live camera frame/photo/recording harness.
- 2026-05-19: added installed-platform unsupported transport checks for raw TCP TLS, TCP Fast Open, and QUIC.
- 2026-05-19: added installed-platform checks for safe macOS host-control entry points.
- 2026-05-19: added installed-platform checks for basic macOS services and Bluetooth cache shape.
- 2026-05-19: added no-prompt media-library query and missing-asset request coverage through the installed macOS platform.
- 2026-05-19: added no-prompt location, motion, camera unsupported-control, and push non-bundled coverage through the installed macOS platform.
- 2026-05-19: added main-thread host coverage for permission status and network status through the installed macOS platform.
- 2026-05-19: guarded UserNotifications access for non-bundled host-test processes.
- 2026-05-19: added main-thread WebView harness coverage for live hidden `WKWebView` load and JavaScript execution.
- 2026-05-19: added installed-platform TCP/UDP loopback verification through the shared Apple socket backend.
- 2026-05-19: added native `NSURLSession` loopback HTTP GET verification through the installed macOS platform.
- 2026-05-19: added live Keychain-backed secure-storage verification through the installed macOS platform.
- 2026-05-19: replaced panicking host mutex locks with poison recovery and added callback fanout tests.
- 2026-05-19: added macOS host ABI coverage for shared Apple WebView services.
- 2026-05-19: added macOS host ABI coverage for shared Apple push services.
- 2026-05-19: added macOS host ABI coverage for shared Apple media-library services.
- 2026-05-19: added macOS host ABI coverage for shared Apple location/motion services.
- 2026-05-19: installed the macOS platform at host initialization and routed raw macOS touch callbacks through the shared input tracker.
