# oxide-host-macos `src/macos/app.m`

## Intention and purpose
- Provide the AppKit, Metal view, and native macOS service shims consumed by the Rust macOS host and platform crate.

## Relation to the rest of the code
- `oxide-host-macos/src/lib.rs` owns renderer and scene state, while this file owns native event delivery and OS framework calls.
- `oxide-platform-macos` calls the exported platform-service functions for URL opening, device caps, location, motion availability, media-library, push, WebView, and network status. The macOS host build compiles the shared Apple HTTP and secure-storage sources alongside this AppKit shim to provide `oxide_host_http_*` and `oxide_secure_storage_*`.

## Entry points list
- `macos_host_start()`
  Starts the AppKit application shell.
- `macos_open_system_settings()` and `macos_open_external_url(...)`
  Bridge platform URL actions into `NSWorkspace`.
- `macos_clipboard_get(...)`, `macos_clipboard_set(...)`, and `macos_free(...)`
  Bridge pasteboard text through native-owned buffers consumed by `MacPlatform`.
- `macos_network_status(...)`, `macos_set_network_status_callback(...)`, and `macos_start_network_monitor()`
  Expose Network.framework status snapshots and subscriptions to Rust.
- `macos_permission_status(...)`, `macos_permission_request(...)`, and `macos_set_permission_callback(...)`
  Expose native macOS permission status/request behavior to Rust.
- `oxide_host_location_*`
  Expose the shared Apple location ABI used by `AppleLocationService`.
- `oxide_host_motion_*` and `macos_motion_available()`
  Expose the shared Apple motion ABI as a no-provider macOS path.
- `oxide_media_*`
  Expose the shared Apple media-library ABI used by `AppleMediaLibraryManager`, including structured return codes for permission, I/O/export failure, invalid input, unsupported conversion, and missing assets.
- `oxide_host_push_*`
  Expose the shared Apple push ABI used by `ApplePushManager`.
- `oxide_cam_*` and `oxide_host_set_camera_*`
  Expose the shared Apple camera ABI used by `AppleCameraManager`, including structured negative return codes for permission, missing-device/session, busy, invalid, I/O, and unsupported cases.
- `oxide_host_emit_perm(...)`
  Publishes shared Apple permission updates, including CoreBluetooth authorization changes from the shared Bluetooth bridge.
- `oxide_web_view_*`
  Expose the shared Apple WebView ABI used by `AppleWebViewService`, including structured negative return codes for invalid input, unavailable WebKit setup, busy duplicate view handles, missing handles, script failures, and result-copy I/O failures.
## Logic narrative
- Network status uses one persistent `nw_path_monitor_t` on a utility queue.
- Each path update atomically stores connectivity and active interface bits, then invokes the Rust callback if one is installed.
- Interface bits match the shared `oxide-platform-apple` constants so Wi-Fi, cellular, and wired decoding is consistent across Apple hosts.
- Secure storage is provided by the shared Apple native Keychain source, which is compiled into this host and exports the `oxide_secure_storage_*` ABI consumed by `AppleSecureStorage`.
- Clipboard reads initialize their output parameters before touching `NSPasteboard`, return success for intentional empty strings, and allocate UTF-8 buffers only for non-empty data.
- HTTP GET is provided by the shared Apple native URLSession source, which is compiled into this host and exports the `oxide_host_http_*` ABI consumed by `AppleHttpClient`.
- Location uses a CoreLocation manager on the main queue, copies delegate updates into the shared Apple location ABI, and synchronizes last-sample reads through the main queue.
- Motion exports the shared ABI but reports unavailable because `CMAltimeter` is explicitly unavailable on macOS in the current SDK.
- Media library uses PhotoKit to fetch image/video assets, normalize image requests to JPEG or BGRA buffers, and export non-file-backed video assets to temporary `.mov` files when necessary.
- Media-library host return codes distinguish Photos permission denial, invalid ABI inputs, allocation/copy/export I/O failures, unsupported video export paths, and missing assets.
- Push uses APNs registration and UserNotifications delegate callbacks to publish device tokens and notification user-info dictionaries through the shared Apple push ABI. Badge updates use `NSApplication.dockTile.badgeLabel`, and delivered notification clearing also clears the Dock badge.
- UserNotifications access goes through a guarded notification-center helper. Non-bundled host-test processes report notifications as not-determined instead of allowing `UNUserNotificationCenter` to abort the process.
- Camera uses an AVFoundation `AVCaptureSession` with `AVCaptureVideoDataOutput` for NV12 frame delivery, optional `AVCaptureAudioDataOutput` for audio samples, `AVCaptureMovieFileOutput` for recording, and next-frame NV12 extraction for photo capture. Visible preview rendering stays Oxide-owned through app-visible frames.
- Camera availability for `Platform::capabilities()` is based on AVFoundation capture-device discovery, so camera bits are not advertised on a Mac with no available camera.
- Bluetooth is compiled from `oxide-platform-apple/src/apple/bluetooth.m`; this host links CoreBluetooth and provides the shared permission callback symbol used by the bridge.
- WebView uses hidden `WKWebView` instances owned on the main queue. Navigation delegate callbacks publish load-finished/load-failed events through Rust, script execution waits for WebKit completion without blocking renderer frame encode/present paths, and close stops loading plus removes the hidden view.
- WebView create returns busy for duplicate view IDs, script execution returns not-found for missing handles, and script-result copy allocation failures return I/O so Rust does not confuse them with JavaScript `undefined`.
- Permissions map Oxide domains onto macOS frameworks: UserNotifications, CoreLocation, AVFoundation, Contacts, CoreBluetooth, and Photos. Requests emit callback snapshots through the shared Apple permission raw-code ABI.
- Haptics use `NSHapticFeedbackManager` on the main queue instead of silently dropping feedback requests.
- The Metal view prepares the Rust/Oxide frame before acquiring a `CAMetalDrawable`, then uses timeout-capable late drawable acquisition and cancels the prepared frame when no drawable is available so drawable pressure skips a frame instead of blocking indefinitely. The Rust host retains the prepared damage on that skip so the next drawable-backed submit retries the same dirty region.

## Preconditions and postconditions
- AppKit, AVFoundation, Contacts, CoreBluetooth, CoreLocation, CoreMedia, Network.framework, Photos, Security.framework, UserNotifications.framework, and WebKit.framework must be linked by `build.rs`.
- The app bundle must include privacy usage strings for camera, microphone, contacts, Bluetooth, location, motion, and Photos before those prompts are exercised.
- Network callbacks must not assume delivery on the main thread.
- Loaded secure-storage buffers must be released through `oxide_secure_storage_free_data`.

## Edge cases and failure modes
- A missing Keychain item returns the shared not-found status accepted by `AppleSecureStorage` through the shared Apple native source compiled into the host.
- Clipboard reads return failure for null output parameters, missing pasteboard strings, or encoding failure. Empty strings return success with a zero-length result so Rust can distinguish them from missing data.
- HTTP returns the shared native error codes consumed by `AppleHttpClient`, including response-too-large and main-thread busy, through the shared Apple native source compiled into the host.
- Disconnected network paths clear the interface bitmask before publishing.
- Motion permission returns denied because this macOS host has no matching motion permission source.
- Motion service start returns unavailable because this macOS host has no matching motion sample provider.
- Media-library calls return permission denied until Photos authorization is granted, and missing assets return not-found through the shared ABI.
- Push token queries return an empty result until APNs registration succeeds; live token delivery depends on bundle entitlement and provisioning.
- Notification status/register/clear calls become no-ops or not-determined status in non-bundled host-test processes where UserNotifications cannot create a current notification center.
- Camera startup returns permission-denied until macOS camera permission is authorized and not-found until a capture device/session is available. Native preview-layer startup returns unsupported by design. Zoom is unsupported because the AVFoundation zoom property is unavailable on macOS, and flash/torch only accept off because macOS capture devices do not expose the iOS-style controls.
- Bluetooth live scan/connect/GATT/advertising behavior depends on hardware, permissions, and CoreBluetooth state restoration availability.
- WebView create rejects empty or scheme-less URLs, unavailable WebKit setup, and duplicate view IDs distinctly. Script execution reports missing handles after close, JavaScript evaluation failures, and result-copy I/O failures distinctly.
- If Network.framework monitor creation fails, `macos_network_status` reports failure to Rust.
- If `CAMetalLayer.nextDrawable` times out under drawable pressure, the host cancels the prepared frame and returns without submitting stale work. The dirty region remains pending and `frame_dirty` is rearmed for retry.

## Testing and benchmarks
- Linkage and host lifecycle are covered by `cargo check -p oxide-host-macos --features host-testing --tests --locked` and `cargo test -p oxide-host-macos --features host-testing --tests --locked`.
- HTTP GET behavior is covered by the host harness through a loopback TCP server, the installed macOS platform, the shared native `NSURLSession` bridge, and the shared Apple response-copy/free wrapper.
- Keychain save/load/delete behavior is covered by the host harness through the installed macOS platform, shared Apple secure-storage wrapper, and shared Apple native Keychain bridge.
- Permission status snapshots, network status snapshots/subscription callbacks, no-prompt location/motion/camera/push behavior, and live hidden-WebView local navigation/script execution are covered by the main-thread WebView harness.
- `metal_drawable_lifetime_tests.rs` statically enforces late drawable acquisition, timeout-capable `nextDrawable`, cancellation of prepared frames when acquisition fails, and pending-damage retention for the retry frame.

## Changelog
- 2026-06-01: retained prepared-frame damage across macOS drawable timeout or submit failure so dirty regions retry after pressure.
- 2026-06-01: made macOS drawable acquisition timeout-capable and documented the skip-on-pressure frame contract.
- 2026-05-19: moved the native Keychain secure-storage bridge out of `app.m` and into shared `oxide-platform-apple`.
- 2026-05-19: moved the native HTTP URLSession bridge out of `app.m` and into shared `oxide-platform-apple`.
- 2026-05-31: verified no-prompt location, motion no-provider, camera unsupported-control, and push non-bundled paths through the installed macOS platform after removing the public native-preview contract.
- 2026-05-19: guarded UserNotifications access so non-bundled host-test processes report notification unavailability instead of aborting.
- 2026-05-19: verified permission status snapshots, network status snapshots/subscription callbacks, and live hidden-WebView navigation/script execution through a main-thread host harness.
- 2026-05-19: verified the native HTTP GET ABI with a loopback `NSURLSession` host test.
- 2026-05-19: verified the Keychain secure-storage ABI with a live host round-trip/delete test.
- 2026-05-19: made clipboard reads initialize outputs and preserve successful empty strings.
- 2026-05-19: preserved structured WebView create/script return codes for duplicate handles and result-copy I/O failures.
- 2026-05-19: added structured macOS media-library return codes for PhotoKit query/load/export paths.
- 2026-05-19: added macOS camera availability reporting and structured native camera return codes.
- 2026-05-19: added macOS AVFoundation camera ABI support for NV12 frame streams, audio samples, recording, photo events, format enumeration, and CoreMedia linkage while rejecting the system preview-layer path.
- 2026-05-19: added shared CoreBluetooth bridge linkage, Bluetooth permission publication, and AppKit haptic feedback.
- 2026-05-19: added hidden WKWebView create/load/script/close ABI support.
- 2026-05-19: added APNs/UserNotifications push ABI support and Dock badge clearing.
- 2026-05-19: added PhotoKit-backed media-library query, image, BGRA, and video file-path ABI support.
- 2026-05-19: added CoreLocation host ABI support, macOS privacy usage strings, and explicit no-provider motion ABI behavior.
- 2026-05-19: added Keychain secure storage, shared HTTP GET bridge, native permission status/request bridging, and Network.framework status snapshots with interface-class reporting.
