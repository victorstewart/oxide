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

## Logic narrative
- The helper extracts the Objective-C `handleFailure` method body between stable method markers.
- The test confirms the forced TCP/TLS branch appears before the generic `_quicParameters` retry.
- It also checks both request sources for forced mode: the config field and the environment override.

## Preconditions and postconditions
- The Objective-C method markers must remain present in `network.m`.
- A passing test means the source keeps forced TCP/TLS retries on TLS parameters before any generic QUIC retry can run.

## Edge cases and failure modes
- If the bridge method is renamed or moved, the test fails at marker lookup so the invariant must be revalidated.
- The test does not prove a real Network.framework connection succeeds; it only guards the retry state transition in source.

## Testing and benchmarks
- Run with `cargo test -p oxide-platform-ios --test network_bridge_tests`.

## Changelog
- 2026-05-17: added coverage for forced TCP/TLS retry routing in `network.m`.
