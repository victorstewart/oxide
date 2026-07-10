# platform-ios `tests/network_bridge_tests.rs`

## Intention and purpose
- Protect Network.framework Objective-C bridge invariants that are hard to exercise from a host-independent Rust unit test.
- Catch regressions where caller-selected transport policy changes during native retry handling.

## Relation to the rest of the code
- Reads `crates/platform-ios/src/ios/network.m` as source text.
- Complements native bridge compile checks and the higher-level iOS networking runtime.
- Focuses on control-flow invariants at the FFI boundary rather than opening real network sockets.

## Entry points list
- `forced_tcp_tls_retries_stay_on_tls_parameters()`
  Verifies that `NametagQuicConnection::handleFailure` checks forced TCP/TLS mode before the generic QUIC retry branch and schedules `_tlsParameters`.
- `network_bridge_enables_ticket_resumption_on_public_security_options()`
  Verifies that the Objective-C bridge enables TLS tickets, TLS resumption, and the TLS 1.3 min/max setters while avoiding private early-data setters.
- `network_bridge_configures_tcp_tls13_fast_open_and_early_writes()`
  Verifies that the TCP/TLS fallback path asks for TLS 1.3 only, enables public Network.framework fast-open eligibility, enables TCP Fast Open, and routes sends through the writable-connection wait path instead of requiring `ready` first.
- `network_bridge_consumes_one_receive_permit_before_each_frame_pop()`
  Verifies that `popReceived` waits on the receive semaphore before its sole queue removal and decrements queued-byte accounting with the removed frame.
- `network_bridge_bounds_receive_queue_by_frames_and_bytes()`
  Verifies that admission checks the 64-frame and 32-MiB budgets before copying or appending a frame, and closes on overflow.
- `network_bridge_uses_one_monotonic_deadline_for_send()`
  Verifies that send readiness and completion share one `CLOCK_MONOTONIC` deadline instead of receiving independent full timeout budgets.
- `network_bridge_exposes_nonblocking_tri_state_receive_contract()`
  Freezes the public header's `-1/0/1` values, symbol signature, zero-timeout pop, output length handling, terminal close path, and build-script header tracking.
- `network_bridge_marks_only_terminal_connection_events_closed()`
  Verifies that failed/cancelled states close only after retries, waiting remains nonterminal, and receive error/EOF closes the session.
- `network_bridge_applies_rust_quic_transport_fields()`
  Verifies that Rust's QUIC idle timeout and UDP payload size fields are applied to Network.framework QUIC options.
- `network_bridge_configures_client_keepalive_for_quic_and_tcp_tls()`
  Verifies that TCP/TLS fallback receives TCP keepalive options and ready QUIC connections receive a QUIC keepalive interval through Network.framework metadata.
- `network_bridge_leaves_path_migration_and_ticket_cache_unpinned()`
  Verifies that the bridge does not pin a required interface, fixed local endpoint, or per-connection privacy context that would undermine path migration or session cache reuse.

## Logic narrative
- The helper extracts the Objective-C `handleFailure` method body between stable method markers.
- The test confirms the forced TCP/TLS branch appears before the generic `_quicParameters` retry.
- It also checks both request sources for forced mode: the config field and the environment override.
- The ticket/resumption test extracts `configure_sec_options` and checks only public Security.framework options.
- The TCP/TLS fast-open test extracts the secure-TCP parameter block and send method to make sure early-data eligibility is not configured without a matching pre-ready write path.
- The receive-permit test proves the semaphore wait text precedes the only `firstObject` removal in `popReceived`, preventing a second call from consuming an old permit after a fast-path pop.
- The queue-budget test proves both limits are checked before the frame copy and append, byte accounting grows and shrinks with the queue, and overflow reaches the connection close path.
- The deadline test proves send creates one absolute monotonic deadline, passes it through readiness polling, and computes the completion wait from the same deadline.
- The tri-state test reads both the public header and implementation to keep the Rust-facing ABI and native behavior aligned.
- The terminal-state test distinguishes recoverable Network.framework waiting from failed/cancelled events and verifies receive-loop terminal paths.
- The keepalive test extracts both the TCP/TLS parameter block and the ready-state QUIC metadata path because Network.framework exposes keepalive at different phases for those transports.
- The migration/cache test scans the whole source for Network.framework calls that would constrain interface choice or isolate TLS session cache state.

## Preconditions and postconditions
- The Objective-C method markers must remain present in `network.m`.
- A passing test means the source keeps forced TCP/TLS retries on TLS parameters before any generic QUIC retry can run.
- A passing ticket/resumption test means the bridge keeps asking Security.framework to issue/cache resumable TLS sessions for future QUIC/TCP-TLS connections and keeps the TLS 1.3 protocol setters in the shared security-options path.
- A passing TCP/TLS fast-open test means the fallback path remains TLS 1.3-only and keeps the public Network.framework hooks required for TCP Fast Open and TLS early-data attempts.
- Passing receive-accounting tests mean complete frames cannot grow the queue beyond either budget and each normal queue removal consumes the permit emitted for that frame.
- A passing deadline test means readiness and completion cannot each consume the caller's entire timeout.
- Passing tri-state tests mean polling can distinguish frame, idle, and terminal without changing the blocking receive entry point.
- A passing keepalive test means the native bridge keeps client connections active against the server on both the QUIC path and TCP/TLS fallback path.
- A passing migration/cache test means the bridge leaves Network.framework free to handle path changes and share in-process TLS session cache state in the default privacy context.

## Edge cases and failure modes
- If the bridge method is renamed or moved, the test fails at marker lookup so the invariant must be revalidated.
- The test does not prove a real Network.framework connection succeeds; it only guards the retry state transition in source.
- Source-contract tests do not exercise scheduler timing or live overflow; the Objective-C syntax build provides the native compile gate while higher-level transport integration exercises live request/response behavior.
- The early-data guard stays negative for `sec_protocol_options_set_tls_early_data_enabled` because the installed public SDK headers do not expose that Security.framework setter; the supported TCP/TLS route is guarded through Network.framework fast-open APIs instead.

## Testing and benchmarks
- Run with `cargo test -p oxide-platform-ios --test network_bridge_tests`.

## Changelog
- 2026-07-10: added tri-state receive ABI and terminal connection-state contract coverage.
- 2026-07-10: added source-contract coverage for receive permits, frame/byte queue limits, fail-closed overflow, and one monotonic send deadline.
- 2026-06-11: added guards for TLS 1.3-only TCP/TLS, public fast-open/early-data eligibility, and the pre-ready TCP/TLS write path.
- 2026-06-11: added guards for TLS tickets/resumption, Rust QUIC option application, client keepalive, public-API-only early-data handling, and path/cache unpinning.
- 2026-05-17: added coverage for forced TCP/TLS retry routing in `network.m`.
