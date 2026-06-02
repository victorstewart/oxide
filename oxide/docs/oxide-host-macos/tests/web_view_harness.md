# oxide-host-macos `tests/web_view_harness.rs`

## Intention and purpose
- Verify macOS main-thread backend services through the installed `MacPlatform`: redraw/refresh/idle-timer/IME host controls, paths, time/device caps, capabilities shape, telephony no-provider, haptics dispatch, Bluetooth cache shape, permission-status snapshots, network-status snapshots/subscription callbacks, no-prompt location/motion/camera/push/media-library behavior, opt-in live location/camera/media behavior on pre-authorized services/hardware/libraries, real hidden `WKWebView` lifecycle behavior, and real WebKit JavaScript evaluation.
- Keep WebView verification off ordinary libtest worker threads because WebKit/AppKit work must run with the process main run loop available.

## Relation to the rest of the code
- Exercises `oxide-host-macos` initialization and process-global platform installation.
- Reaches `MacPlatform::request_redraw()`, refresh/idle-timer/IME controls, `MacPlatform::paths()`, `MacPlatform::time()`, `MacPlatform::device_caps()`, `MacPlatform::capabilities()`, `MacPlatform::telephony()`, `MacPlatform::haptics()`, `MacPlatform::bluetooth()`, `MacPlatform::permissions()`, `MacPlatform::network_status()`, `MacPlatform::location()`, `MacPlatform::motion()`, `MacPlatform::camera()`, `MacPlatform::push()`, `MacPlatform::media_library()`, `MacPlatform::web_view_service()`, the shared Apple service wrappers, and the AppKit/WebKit `oxide_web_view_*` host ABI together.
- Complements shared `oxide-platform-apple` ABI tests by proving the macOS native host path loads content and executes script.

## Entry points list
- `main()`
  Custom harness entry point registered in `oxide-host-macos/Cargo.toml` with `harness = false`.
- `harness::run()`
  Initializes the macOS host, verifies status and no-prompt service paths, creates hidden WebViews for temporary local HTML documents, waits for load completion, evaluates JavaScript result/error cases, closes the views, removes the temporary documents, and resets the host harness.

## Logic narrative
- The harness starts on the process main thread, so `MacDispatchMainSync` can execute WebKit work directly without deadlocking on libtest worker scheduling.
- Basic service checks verify redraw, refresh toggle, idle-timer toggle, IME no-op controls, simulation reporting, standard directories, monotonic time, sane device caps, coherent capability bits, macOS telephony no-provider behavior, haptic dispatch, and Bluetooth cached-entry shape if any exist.
- Permission status checks cover every Oxide permission domain without issuing prompts; notification status degrades to not-determined when the custom test process has no app-bundle identity.
- Network status checks read the current status and verify the immediate subscriber callback shape without requiring a specific machine connectivity state.
- Location checks confirm no cached sample is reported before location startup and that accuracy updates can be applied without prompting.
- When `OXIDE_MACOS_LIVE_LOCATION=1` is set on a pre-authorized host with enabled Location Services, the harness starts CoreLocation, requests one update, verifies the update shape, verifies `last()` and history caching, and stops updates.
- Motion checks confirm the macOS no-provider path returns explicit unsupported and does not enter a running state.
- Camera checks confirm macOS zoom, flash-on, and torch-on paths return explicit unsupported while flash-off and torch-off are accepted. The harness also verifies photo capture and recording start return structured missing-session errors before a stream exists. If camera permission is not authorized, the harness verifies camera stream startup returns a structured permission error without issuing a prompt.
- When `OXIDE_MACOS_LIVE_CAMERA=1` is set on a pre-authorized host with available camera hardware, the harness starts the real AVFoundation-backed stream, verifies an Oxide-owned NV12 frame, captures a high-speed photo from the preview stream, records a short audio-disabled movie, verifies the output file, and removes it.
- Push checks confirm non-bundled host-test registration, badge, clear, and token query paths do not abort and do not invent an APNs token.
- Media-library checks confirm PhotoKit image/video queries either return valid bounded assets when authorized or structured permission-denied errors when unauthorized, load a real image thumbnail when an authorized image asset is already available, and verify missing synthetic asset requests map to permission-denied or not-found rather than unknown failures.
- When `OXIDE_MACOS_LIVE_MEDIA=1` is set on a pre-authorized Photos library with at least one image and one video, the harness loads a display image and resolves or exports a video file path through the installed platform.
- Temporary `file://` HTML documents define distinct `window.oxideValue` values from DOM content.
- WebView lifecycle callbacks are delivered through separate test-local channels to verify independent callback routing for concurrent hidden views.
- The harness pumps `CFRunLoopRunInMode(kCFRunLoopDefaultMode, ...)` while waiting for WebKit load completion.
- After `LoadFinished`, the harness evaluates string, number, empty-string, undefined, JSON stringification, throwing-script, and invalid-empty-script cases. It also verifies idempotent close, closed-handle not-found errors, and that closing one hidden view does not break a sibling view.

## Preconditions and postconditions
- Requires macOS and the `host-testing` feature.
- Requires WebKit and the native macOS permission/network frameworks to be available in the host process.
- On success, the hidden WebView is closed, the temporary HTML file is removed, and the process-global host platform is reset.

## Edge cases and failure modes
- `LoadFailed` fails the harness with the platform error returned by the shared WebView callback bridge.
- Missing load callbacks time out instead of hanging indefinitely.
- A pending future from `execute_script` fails the harness because the current shared Apple WebView implementation completes synchronously after the native WebKit call returns.
- Closing a hidden WebView twice must be harmless, and script execution after close must return not-found.
- Disconnected network-status snapshots must not report active interface bits; connected snapshots are allowed to vary by host machine.
- The no-prompt checks deliberately avoid settings/URL launching, clipboard mutation, authorized camera activation, location start/request-once, Photos authorization prompts, APNs entitlement flows, and Bluetooth hardware operations.
- The live location path is opt-in so normal test runs do not activate Location Services or create privacy prompts.
- The live camera path is opt-in so normal test runs do not activate camera hardware or create privacy prompts.
- The live media path is opt-in so normal test runs do not require a specific Photos library shape or force video export.

## Concurrency and memory behavior
- The harness avoids shared mutable callback buffers by sending owned `NetworkStatus` and `WebViewEvent` values through channels.
- Location callbacks send owned location events through a channel; live updates are stopped before the host reset when the opt-in path is enabled.
- Camera callbacks send owned frame/photo/recording events through channels; the live stream and recording handles are stopped before host reset when the opt-in path is enabled.
- Video extraction validation removes only temporary files created by the macOS media export path and never deletes original Photos library files.
- Both WebViews are closed before host reset so native `WKWebView` teardown runs through the production close path.

## Performance notes
- This is a backend service smoke test, not a renderer benchmark.
- It does not touch renderer frame encode/present hot paths.

## Feature flags and cfgs
- Registered as a custom test target with `required-features = ["host-testing"]`.
- Contains a no-op `main` for non-macOS/non-host-testing cfgs.

## Testing and benchmarks
- Run with `cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`.
- Run the strict live location path on an already-authorized macOS host with Location Services enabled using `OXIDE_MACOS_LIVE_LOCATION=1 cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`.
- Run the strict live camera path on an already-authorized macOS host with `OXIDE_MACOS_LIVE_CAMERA=1 cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`.
- Run the strict live media path on an already-authorized macOS host with image and video assets using `OXIDE_MACOS_LIVE_MEDIA=1 cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`.

## Changelog
- 2026-05-19: added an opt-in live CoreLocation path for pre-authorized macOS hosts covering start, request-once, update callback, last reading, history, and stop.
- 2026-05-19: expanded installed-platform WebView coverage for invalid URL rejection, concurrent hidden views, independent callbacks, script result/error variants, idempotent close, closed-handle errors, and sibling-view survival.
- 2026-05-19: added authorized image-thumbnail extraction and an opt-in live media image/video extraction path through the installed macOS platform.
- 2026-05-19: added no-prompt camera missing-session and startup permission-error verification plus an opt-in live camera frame/photo/recording path for pre-authorized macOS hardware.
- 2026-05-19: added installed-platform checks for safe host controls: redraw, refresh toggle, idle-timer toggle, IME no-ops, and simulation reporting.
- 2026-05-19: added installed-platform checks for paths, monotonic time, device caps, capabilities, telephony no-provider, haptics dispatch, and Bluetooth cache shape.
- 2026-05-19: added media-library no-prompt PhotoKit query and missing-asset request verification through the installed macOS platform.
- 2026-05-19: added no-prompt location, motion, camera unsupported-control, and push non-bundled verification through the installed macOS platform.
- 2026-05-19: added permission-status and network-status verification through the installed macOS platform.
- 2026-05-19: added live hidden `WKWebView` load and JavaScript execution coverage through the installed macOS platform.
