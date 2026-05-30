# oxide-host-macos `tests/headless_harness.rs`

## Intention and purpose
- Verify the macOS host can initialize, drive headless frames, install the process-global platform, and expose stable callback fanout without launching a full AppKit UI test.
- Keep host-global callback tests deterministic even when Cargo runs tests with multiple threads.

## Relation to the rest of the code
- Exercises exported Rust host entry points from `oxide-host-macos/src/lib.rs`.
- Confirms the host installs `oxide-platform-macos` into `oxide-platform-api`.
- Verifies callback registration functions used by `src/macos/app.m` can forward touch, pointer, key, text, pinch, and rotate samples into Rust.

## Entry points list
- `headless_host_smoke()`
  Resets the host harness, calls `macos_app_init`, drives several headless frames, verifies renderer draw output, and confirms reset clears the current platform.
- `host_callbacks_forward_registered_events()`
  Registers every host callback family, emits one event for each family, and verifies each registered callback fires once.
- `host_secure_storage_round_trips_live_keychain()`
  Initializes the macOS host platform and verifies the shared Apple secure-storage wrapper saves, loads, deletes, and confirms deletion through the live Keychain ABI.
- `host_http_get_fetches_loopback_response()`
  Initializes the macOS host platform, starts a loopback HTTP server, and verifies `MacPlatform::http()` fetches status, body, content type, and final URL through the native host ABI.
- `host_networking_tcp_connects_and_reads_loopback_response()`
  Initializes the macOS host platform, connects to a loopback TCP echo server through `MacPlatform::networking()`, writes a request, and verifies the shared Apple socket reader reports the response.
- `host_networking_tcp_keepalive_connects_and_reads_loopback_response()`
  Initializes the macOS host platform, connects to a loopback TCP echo server with `TcpOptions::keepalive`, writes a request, and verifies the shared Apple socket reader reports the response.
- `host_networking_udp_sends_and_reads_loopback_packet()`
  Initializes the macOS host platform, binds a shared Apple UDP socket through `MacPlatform::networking()`, sends to a loopback UDP echo server, and verifies the response packet.
- `host_networking_rejects_unsupported_transport_options()`
  Initializes the macOS host platform and verifies raw TCP TLS, TCP Fast Open, and QUIC requests return `PlatformError::Unsupported` through `MacPlatform::networking()`.

## Logic narrative
- The smoke test starts from a clean harness state and asserts no process-global platform is registered before initialization.
- After initialization, the test drives frames through both ordinary headless and explicit-null-drawable entry points because the host supports both call sites.
- The callback test uses static atomic counters because callback functions are plain `extern "C" fn` pointers and cannot capture local state.
- The secure-storage test creates a unique key from process id plus wall-clock nanoseconds, deletes any stale value, performs a save/load/delete/load round trip, and then resets the host harness.
- The HTTP test serves one response from a local `TcpListener` and fetches it through `oxide_platform_api::HttpRequest`, proving the `NSURLSession` bridge and shared Apple response wrapper work without external network access.
- The TCP tests serve one response from a local `TcpListener`, write through the installed platform's shared Apple connection object, and wait for a connection callback carrying the response bytes. The keepalive variant enables Apple socket keepalive options before the connection is handed to the reader thread.
- The UDP test binds a local echo socket, sends through the installed platform's shared Apple UDP object, and waits for a packet callback carrying the response bytes.
- The unsupported transport test builds options that would otherwise target loopback addresses, then verifies unsupported semantics are rejected before creating a connection.
- A process-local test mutex serializes the harness tests because the host callback slots and current-platform registry are process-global.

## Preconditions and postconditions
- Tests require macOS plus the `host-testing` feature for real assertions.
- On non-macOS or without `host-testing`, the placeholder test does nothing.
- The callback test unregisters every callback family before returning.
- The secure-storage test deletes its unique Keychain item before and after verification.
- The HTTP test joins the loopback server thread before resetting the harness.
- The TCP, keepalive TCP, and UDP tests close shared Apple socket handles and join their loopback server threads before resetting the harness.
- The unsupported transport test resets the harness after confirming each rejection path.

## Edge cases and failure modes
- The smoke test verifies the no-drawable path remains valid.
- Callback fanout is checked independently from the scene router so registration bugs are visible even if the router is not active.
- Secure-storage verification reaches the same installed platform object app code uses, not test-local secure-storage stubs.
- HTTP verification reaches the same installed platform object app code uses, not test-local HTTP stubs.
- TCP/UDP networking verification reaches the same installed platform object app code uses, not direct `AppleSocketNetworking` construction in the test.
- Unsupported raw transport verification reaches the same installed platform object app code uses, proving unsupported TLS/QUIC/TFO behavior is a macOS host contract rather than only a shared-crate unit test.
- Socket callback waits have bounded timeouts so reader-thread regressions fail the test instead of hanging indefinitely.
- Test serialization prevents callback registration from racing host initialization.

## Concurrency and memory behavior
- Static atomic counters record callback delivery.
- TCP and UDP networking callbacks send owned platform events over test-local channels, avoiding shared mutable callback buffers.
- The test mutex uses poison recovery so a failed test does not permanently poison later harness cleanup.

## Performance notes
- The tests are smoke and callback-contract checks, not renderer benchmarks.
- They do not alter renderer frame encode/present hot paths.

## Feature flags and cfgs
- Full coverage is compiled under `all(target_os = "macos", feature = "host-testing")`.
- A no-op placeholder test preserves portability for other host targets.

## Testing and benchmarks
- Run with `cargo test -p oxide-host-macos --features host-testing --tests --locked`.

## Examples
```rust
macos_set_touch_callback(Some(touch_cb));
macos_emit_touch(7, 0, 12.0, 13.0, 100);
```

## Changelog
- 2026-05-19: added installed-platform TCP keepalive loopback coverage through the shared Apple socket backend.
- 2026-05-19: added installed-platform unsupported transport coverage for raw TCP TLS, TCP Fast Open, and QUIC.
- 2026-05-19: added installed-platform TCP/UDP loopback coverage through the shared Apple socket backend.
- 2026-05-19: added native loopback HTTP GET coverage through the installed macOS platform.
- 2026-05-19: added live Keychain-backed secure-storage round-trip coverage through the installed macOS platform.
- 2026-05-19: added callback fanout coverage for touch, pointer, key, text, pinch, and rotate callbacks.
