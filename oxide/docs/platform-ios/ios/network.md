# platform-ios `src/ios/network.m`

## Intention and purpose
- Provide the native Network.framework bridge used by the iOS platform crate for transport and reachability services.

## Relation to the rest of the code
- `oxide-platform-ios/src/lib.rs` calls the exported `oxide_host_net_*` symbols from this file.
- `src/ios/network.h` publishes the tri-state nonblocking retained-session receive symbol while the existing blocking receive ABI remains private to downstream Rust declarations.
- Reachability snapshots are decoded in Rust through `oxide-platform-apple` so iOS and macOS share path-kind semantics.
- HTTP symbols are now compiled from `oxide-platform-apple/src/apple/http.m` so iOS and macOS share the native `NSURLSession` bridge.

## Entry points list
- `oxide_host_net_set_reachability_callback(cb)`
  Installs the Rust reachability callback.
- `oxide_host_net_start_reachability()`
  Starts a persistent `nw_path_monitor_t` and emits the latest snapshot.
- `oxide_host_net_stop_reachability()`
  Stops and releases the monitor.
- `path_kind_for_path(path)`
  Maps active Network.framework interfaces to Oxide's Apple path-kind constants.
- `configure_sec_options(sec_options, tls, server_name, alpn, tls13Only)`
  Configures the shared TLS state for QUIC and TCP/TLS attempts, including ALPN, optional local identity, trust anchors, session tickets, session resumption, and TCP/TLS-only TLS 1.3 min/max enforcement.
- `NametagQuicConnection::configureReadyKeepalive()`
  Applies the configured QUIC keepalive interval after Network.framework has established the connection and made QUIC metadata available.
- `NametagQuicConnection::waitForWritableConnection(deadlineNs)`
  Waits until one absolute monotonic deadline for normal QUIC readiness but allows TCP/TLS fallback writes once the fallback connection exists, which is required for Network.framework fast-open and early-data attempts.
- `NametagQuicConnection::sendBytes(data, length, timeout)`
  Applies one overall monotonic deadline to both connection readiness and asynchronous send completion.
- `NametagQuicConnection::popReceived(timeoutMs)`
  Consumes one receive semaphore permit before removing the corresponding frame from the bounded queue.
- `NametagQuicConnection::drainIncomingBytes()`
  Validates stream frame lengths and admits complete frames only while both the frame-count and queued-byte budgets permit them.
- `nametag_ios_quic_poll_recv(handle, buffer, buffer_len, out_len)`
  Returns frame, idle, or terminal without waiting for future input; queued frames drain before the terminal state is reported.

## Logic narrative
- QUIC connections are created with `nw_parameters_create_quic`, then the bridge configures the underlying Security options and applies the Rust-provided idle timeout and UDP payload size to the `nw_protocol_options_t`.
- TLS session tickets and TLS resumption are enabled through public Security.framework APIs so Network.framework can cache resumable TLS state in the process privacy context.
- TCP/TLS fallback constrains Security.framework to TLS 1.3 by setting both the minimum and maximum TLS protocol version to `tls_protocol_version_TLSv13`.
- TCP/TLS fallback enables TCP Fast Open on the TCP options and top-level Network.framework fast-open on the TLS parameters. Apple documents that top-level flag as the public way to permit fast open or TLS early data for protocols in the stack, so the first TCP/TLS fallback write can run before `ready`.
- The Rust keepalive interval is rounded to seconds before crossing FFI. TCP/TLS fallback receives TCP keepalive options before connect, while QUIC receives `nw_quic_set_keepalive_interval` from `configureReadyKeepalive()` after `nw_connection_state_ready`.
- The bridge deliberately does not create a per-connection `nw_privacy_context_t`, because Network.framework scopes cached TLS sessions to privacy contexts.
- Connection migration stays on Network.framework's default behavior: the bridge does not require a specific interface, prohibit interface classes, or bind a fixed local endpoint.
- The current public Xcode 26.5 Security headers expose early-data metadata but no public `sec_protocol_options` setter for enabling early data, so the bridge does not call the private `sec_protocol_options_set_tls_early_data_enabled` export.
- A send computes one deadline from `CLOCK_MONOTONIC`. Readiness polling and send-completion waiting repeatedly derive only their remaining nanoseconds from that deadline, so the two phases cannot each consume the full caller timeout.
- Each accepted receive frame contributes exactly one semaphore permit. `popReceived` consumes that permit before entering the serial queue and removes at most one frame, preventing a fast-path removal from leaving a stale permit behind.
- The receive queue admits at most 64 frames and 32 MiB. Crossing either budget logs an error, rejects the new frame, cancels the connection, and wakes blocked readiness and receive callers.
- Failed/cancelled connection states become terminal only after retry and fallback paths are exhausted. Network.framework's recoverable `waiting` state remains nonterminal but is marked nonwritable until ready again, while receive error/EOF closes the retained session.
- Tri-state polling reuses `popReceived:0`, preserving the existing semaphore/queue invariant without changing `nametag_ios_quic_recv` blocking behavior.
- The monitor updates cached reachability fields from its serial queue.
- Every path update emits connected/offline state, the preferred path kind, and whether the path is expensive.
- Wired Ethernet is detected directly through `nw_interface_type_wired`, matching the current SDK enum rather than using preprocessor checks that do not apply to enum values.

## Preconditions and postconditions
- Network.framework must be available and linked by the platform-iOS build.
- Rust callbacks must remain ABI-compatible with `(uint32_t status, uint32_t iface, uint8_t expensive)`.
- `queuedReceiveBytes` equals the sum of frame lengths currently held by `receiveBuffer`; retry reset, admission, and removal update both together on the connection's serial queue.
- Every queue removal is preceded by a successful wait on `receiveSignal`.
- Tri-state `out_len` is zero for idle/terminal and is set only after a frame is copied.

## Edge cases and failure modes
- Unknown interface types map to the `Other` path kind.
- A stopped monitor releases native ownership and future starts allocate a fresh monitor.
- Invalid frames and receive-queue overflow fail closed. Already queued valid frames remain available, while the rejected frame is discarded and no further network input is accepted on that connection.
- An undersized tri-state output buffer closes the session and returns terminal; it never reports the dropped frame as idle.
- Saturating deadline construction and dispatch-wait conversion keep extreme timeout values from wrapping.

## Concurrency and memory behavior
- Network.framework callbacks, queue admission, byte accounting, retry resets, and frame removal serialize through `com.oxide.platform.network.quic`.
- Nonblocking polling reads closed state through that same serial queue after a zero-timeout receive attempt.
- Semaphore permits mirror queued frames; terminal receive errors and closure may add one wake-only permit so a blocked caller can observe failure without polling.
- Queue storage is hard-bounded to 32 MiB across at most 64 complete frames, excluding the single bounded frame currently being assembled in `incomingBytes`.

## Performance notes
- Deadline reads and receive accounting are constant-time. No renderer or frame-loop path is involved.
- Frame removal remains bounded by the 64-frame array limit, and overflow closes before copying the rejected frame out of `incomingBytes`.

## Testing and benchmarks
- Compiled by `cargo check -p oxide-platform-ios --locked`.
- Transport retry invariants are covered by `crates/platform-ios/tests/network_bridge_tests.rs`.
- Ticket/resumption, TLS 1.3-only TCP/TLS, public-API-only early-data handling, QUIC option application, keepalive, path/cache unpinning, receive permit accounting, queue budgets, monotonic overall send deadlines, and tri-state polling are guarded by `network_bridge_tests.rs` source-level assertions.

## Changelog
- 2026-07-10: exposed nonblocking frame/idle/terminal polling and made exhausted failures plus receive EOF/error observable as terminal retained sessions.
- 2026-07-10: bounded the receive queue by frames and bytes, paired each frame removal with one semaphore permit, and applied one monotonic overall deadline across send readiness and completion.
- 2026-06-11: constrained TCP/TLS fallback to TLS 1.3, enabled public Network.framework fast-open eligibility for TCP/TLS early data, and allowed the first TCP/TLS fallback write before `ready`.
- 2026-06-11: enabled public TLS ticket/resumption options for QUIC/TCP-TLS attempts, applied Rust QUIC idle/UDP-payload/keepalive fields, and documented why early data and migration remain on public Network.framework behavior.
- 2026-05-19: moved the native HTTP `NSURLSession` bridge out to `oxide-platform-apple/src/apple/http.m`.
- 2026-05-19: wired reachability decoding now checks `nw_interface_type_wired` directly and shares Rust-side path decoding with `oxide-platform-apple`.
