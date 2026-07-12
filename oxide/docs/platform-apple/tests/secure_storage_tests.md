# platform-apple `tests/secure_storage_tests.rs`

## Intention and purpose
- Verify the shared Apple secure-storage wrapper and Network.framework value decoders without touching the user Keychain or requiring live network state.
- Verify the shared Apple HTTP wrapper with test-local C ABI stubs, avoiding live network access.
- Verify the shared Apple location/motion Rust services with test-local host ABI stubs and direct callback trampoline calls.
- Verify the shared Apple media-library Rust service with test-local asset/image/video ABI stubs.
- Verify the shared Apple push manager with test-local APNs/UserNotifications ABI stubs.
- Verify the shared Apple Bluetooth manager with test-local CoreBluetooth ABI stubs and direct event trampoline calls.
- Verify the shared Apple camera manager with test-local AVFoundation ABI stubs and direct frame/audio/record/photo trampoline calls.
- Verify the feature-gated shared Apple WebView service with test-local WebKit ABI stubs.
- Verify shared Apple TCP/UDP socket networking with local loopback sockets.

## Relation to the rest of the code
- Defines test-local `oxide_secure_storage_*` symbols so `AppleSecureStorage` exercises the same C ABI exported by the shared native Keychain bridge used by the iOS and macOS hosts.
- Defines test-local `oxide_host_http_*` symbols so `AppleHttpClient` exercises the same asynchronous event/cancellation ABI used by the iOS and macOS hosts.
- Defines test-local `oxide_host_location_*` and `oxide_host_motion_*` symbols so the shared location/motion services can link without a live Apple host.
- Defines test-local `oxide_media_*` symbols so the shared media-library service can exercise asset paging and host-buffer release without a live Photos library.
- Defines test-local `oxide_host_push_*` symbols and callback registration cells so the shared push manager can exercise token, badge, clearing, and notification fanout without APNs.
- Defines test-local `oxide_ble_*` symbols so the shared Bluetooth manager can exercise scan/connect/read/write/notify/advertise controls without live CoreBluetooth hardware.
- Defines test-local `oxide_cam_*` symbols so the shared camera manager can exercise stream startup, preview-only startup, controls, photo capture, recording, and callback trampolines without live AVFoundation hardware.
- Defines feature-gated test-local `oxide_web_view_*` symbols so the shared WebView wrapper can exercise create, load events, script results, and close behavior without WebKit.
- Checks the network path and interface helpers consumed by `oxide-platform-ios` and `oxide-platform-macos`.
- Checks the permission domain/status helpers consumed by the iOS and macOS permission bridges.

## Entry points list
- `apple_secure_storage_round_trips_c_abi()`
  Saves, loads, overwrites empty values, deletes, and confirms missing-key behavior.
- `apple_http_client_streams_through_c_abi()`
  Verifies response metadata, body chunks, terminal delivery, selected headers, and cancellation through the shared HTTP ABI.
- `apple_path_kinds_decode_to_reachability()`
  Verifies shared Apple path kinds map to Oxide reachability states.
- `apple_network_status_reports_interface_bits()`
  Verifies disconnected status clears interfaces and connected bitmasks preserve active interface classes.
- `apple_permission_codes_round_trip_known_values()`
  Verifies known permission domains/statuses round-trip through the shared raw-code helpers and unknown values fail closed.
- `apple_location_update_trampoline_caches_last_and_history()`
  Verifies host location callbacks populate shared last-sample and bounded-history state.
- `apple_location_region_tracker_emits_enter_and_exit_events()`
  Verifies shared geofence-region state emits enter/exit events from location updates.
- `apple_location_error_trampoline_emits_error_events()`
  Verifies native location error messages become `LocationEvent::Error`.
- `apple_motion_trampoline_caches_history_and_notifies_subscribers()`
  Verifies host motion callbacks populate shared history and subscriber fanout.
- `apple_media_library_queries_assets_with_paging()`
  Verifies shared media asset mapping and offset/limit paging.
- `apple_media_library_loads_image_and_video_data()`
  Verifies shared image, BGRA helper, and video file-path loading through the media ABI.
- `apple_media_library_maps_host_return_codes_to_platform_errors()`
  Verifies native media return codes map to not-found, invalid, I/O, and unsupported `PlatformError` variants.
- `apple_push_manager_registers_caches_token_and_fans_out_notifications()`
  Verifies callback registration, APNs token caching, JSON payload mapping, and subscriber fanout.
- `apple_push_manager_uses_host_token_and_badge_abi()`
  Verifies host token query, badge set/clear, and delivered-notification clearing through the shared push ABI.
- `apple_bluetooth_forwards_controls_and_read_write_notify()`
  Verifies powered-on state, scan start/stop, connect/disconnect, read, write, notify, and advertise calls through the shared Bluetooth ABI.
- `apple_bluetooth_emits_discovery_cache_and_notifications()`
  Verifies discovery, cache updates, connected events, notification events, and cached peripheral snapshots through the shared Bluetooth callbacks.
- `apple_camera_manager_forwards_stream_controls_and_trampolines()`
  Verifies stream startup, settings forwarding, frame/audio callback delivery, focus/zoom/flash/torch controls, and stream teardown through the shared camera ABI.
- `apple_camera_manager_uses_preview_only_without_audio_and_handles_record_photo()`
  Verifies preview-only startup without audio subscribers, photo callback delivery, recording callback delivery, and host recording controls.
- `apple_camera_manager_maps_host_return_codes_to_platform_errors()`
  Verifies native camera return codes map to permission-denied, not-found, busy, invalid, and unsupported `PlatformError` variants.
- `apple_web_view_service_creates_executes_emits_and_closes()`
  Verifies shared WebView create, callback fanout, string/empty/none script result handling, script error mapping, result-copy error mapping, and close idempotence through the WebView ABI.
- `apple_web_view_service_maps_host_create_errors()`
  Verifies native WebView create return codes map unavailable service and busy duplicate-handle failures to structured platform errors.
- `apple_socket_networking_connects_tcp_and_reads()`
  Verifies loopback TCP connect, write, read callback delivery, and close.
- `apple_socket_networking_configures_tcp_keepalive_and_reads()`
  Verifies shared Apple TCP keepalive socket setup still connects, writes, reads, and closes through a loopback socket.
- `apple_socket_networking_binds_udp_sends_and_reads()`
  Verifies UDP bind, send, read callback delivery, and close through a loopback echo socket.
- `apple_socket_networking_rejects_unsupported_transport_options()`
  Verifies TLS and QUIC requests fail explicitly instead of silently downgrading.

## Logic narrative
- The test ABI stores secrets in a process-local `HashMap`.
- The HTTP test ABI borrows event fields for each callback exactly as the native delegate does; the Rust wrapper copies owned event values before returning.
- Loads allocate a copied buffer and rely on `oxide_secure_storage_free_data` to reclaim it, matching the host-owned allocation handoff.
- Network tests stay pure and deterministic by passing raw constants directly into the shared decoder functions.
- Permission tests stay pure by exercising the raw-code conversion helpers directly.
- Location and motion tests call the same Rust trampolines installed into native hosts, but use process-local test ABI symbols instead of CoreLocation or CoreMotion.
- Media tests allocate host-owned image/path buffers and rely on the shared free functions to reclaim them.
- Push tests allocate host-owned token strings, track their lengths, and reclaim them through the same `oxide_host_string_free` ownership path used by native hosts.
- Bluetooth tests allocate host-owned read buffers, reclaim them through the same string-free ownership path, and emit callbacks through the exported Rust trampolines.
- Camera tests allocate host-owned NV12/audio buffers on the stack during trampoline calls and assert that the shared manager copies them into Oxide-owned frame/sample values before callbacks return.
- WebView tests allocate host-owned script result strings and reclaim them through the same `oxide_web_view_free_string` ownership path used by native hosts.
- Socket networking tests use process-local loopback TCP/UDP sockets and do not require external network access.

## Preconditions and postconditions
- Tests must run in an isolated process so the exported ABI symbols do not collide with host-provided symbols.
- After each storage test, the process-local store contains only values written by that test.

## Edge cases and failure modes
- Null pointers and empty keys return a negative ABI error in the test shim.
- Missing deletes return the same idempotent not-found status accepted by `AppleSecureStorage`.
- Empty push token callbacks clear the cached token so a later native token query can repopulate it.
- Bluetooth subscribers and cache are process-global, so tests assert that their local event sink contains expected events instead of relying on exact global event counts.
- Camera subscriber, preview-lease, recording, and photo state is process-global, so each test explicitly stops or drops its stream/recording handles before asserting final call counts.
- WebView scripts that return JavaScript `undefined` map to `Ok(None)`, while empty string results remain `Ok(Some(""))`.
- Socket networking rejects unsupported semantic options rather than accepting a degraded transport.

## Testing and benchmarks
- Run with `cargo test -p oxide-platform-apple --locked`.

## Changelog

- 2026-07-12: added exact/over request-bound coverage and malformed response-FFI pointer/count/length rejection without constructing invalid nonnull pointers.
- 2026-05-19: documented that secure-storage ABI tests cover the same ABI exported by the shared native Keychain bridge.
- 2026-05-19: added shared Apple TCP keepalive socket networking coverage.
- 2026-05-19: added shared Apple WebView return-code mapping tests.
- 2026-05-19: added shared Apple media-library return-code mapping tests.
- 2026-05-19: added shared Apple camera return-code mapping tests.
- 2026-05-19: added shared Apple camera manager tests.
- 2026-05-19: added shared Apple Bluetooth manager tests.
- 2026-05-19: added shared Apple socket networking tests.
- 2026-05-19: added feature-gated shared Apple WebView service tests.
- 2026-05-19: added shared Apple push manager tests.
- 2026-05-19: added shared Apple media-library service tests.
- 2026-05-19: added shared Apple location/motion service tests.
- 2026-05-19: added secure-storage ABI, HTTP ABI, and Apple network/permission decoder coverage.
