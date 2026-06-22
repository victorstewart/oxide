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
- The keepalive test extracts both the TCP/TLS parameter block and the ready-state QUIC metadata path because Network.framework exposes keepalive at different phases for those transports.
- The migration/cache test scans the whole source for Network.framework calls that would constrain interface choice or isolate TLS session cache state.

## Preconditions and postconditions
- The Objective-C method markers must remain present in `network.m`.
- A passing test means the source keeps forced TCP/TLS retries on TLS parameters before any generic QUIC retry can run.
- A passing ticket/resumption test means the bridge keeps asking Security.framework to issue/cache resumable TLS sessions for future QUIC/TCP-TLS connections and keeps the TLS 1.3 protocol setters in the shared security-options path.
- A passing TCP/TLS fast-open test means the fallback path remains TLS 1.3-only and keeps the public Network.framework hooks required for TCP Fast Open and TLS early-data attempts.
- A passing keepalive test means the native bridge keeps client connections active against the server on both the QUIC path and TCP/TLS fallback path.
- A passing migration/cache test means the bridge leaves Network.framework free to handle path changes and share in-process TLS session cache state in the default privacy context.

## Edge cases and failure modes
- If the bridge method is renamed or moved, the test fails at marker lookup so the invariant must be revalidated.
- The test does not prove a real Network.framework connection succeeds; it only guards the retry state transition in source.
- The early-data guard stays negative for `sec_protocol_options_set_tls_early_data_enabled` because the installed public SDK headers do not expose that Security.framework setter; the supported TCP/TLS route is guarded through Network.framework fast-open APIs instead.

## Testing and benchmarks
- Run with `cargo test -p oxide-platform-ios --test network_bridge_tests`.

## Changelog
- 2026-06-11: added guards for TLS 1.3-only TCP/TLS, public fast-open/early-data eligibility, and the pre-ready TCP/TLS write path.
- 2026-06-11: added guards for TLS tickets/resumption, Rust QUIC option application, client keepalive, public-API-only early-data handling, and path/cache unpinning.
- 2026-05-17: added coverage for forced TCP/TLS retry routing in `network.m`.
