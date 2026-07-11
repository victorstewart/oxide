# platform-ios `src/ios/network.h`

## Intention and purpose
- Publish the complete Oxide-owned C ABI for retained iOS Network.framework QUIC/TCP-TLS sessions and reachability snapshots.

## Relation to the rest of the code
- `src/ios/network.m` implements every declared symbol and owns the opaque `OxideQuicHandle` and `OxideReachabilityHandle` lifetimes.
- Downstream Rust FFI consumes the Oxide-named layouts directly and maps native outcomes into its own application transport types.
- `build.rs` watches this header so ABI edits rebuild the native bridge.

## Entry points list
- `OxideQuicConfig`, `OxideQuicRetryPolicy`, `OxideTlsTrustAnchor`, and `OxideQuicTlsConfig`
  Carry caller-selected Network.framework, retry, identity, and trust policy across the C ABI; Oxide does not inject an application-specific retry or forced-transport override.
- `OxideQuicMetrics` and `OxideReachabilityStatus`
  Return transport counters and the latest native path snapshot in fixed-layout records.
- `oxide_ios_quic_connect`, `oxide_ios_quic_wait_ready`, `oxide_ios_quic_send`, `oxide_ios_quic_recv`, `oxide_ios_quic_metrics`, and `oxide_ios_quic_close`
  Own the retained session lifecycle, bounded waits, framed stream transfer, metric snapshots, and native-handle release.
- `int32_t oxide_ios_quic_poll_recv(OxideQuicHandle handle, uint8_t *buffer, size_t buffer_len, size_t *out_len)`
  Polls once without blocking, copies at most one complete queued frame, and returns `OXIDE_IOS_QUIC_POLL_FRAME`, `OXIDE_IOS_QUIC_POLL_IDLE`, or `OXIDE_IOS_QUIC_POLL_TERMINAL`.
- `OXIDE_IOS_QUIC_POLL_FRAME = 1`
  One complete frame was copied and `out_len` is its byte length.
- `OXIDE_IOS_QUIC_POLL_IDLE = 0`
  No frame was queued at the polling instant and the retained session is not terminal.
- `OXIDE_IOS_QUIC_POLL_TERMINAL = -1`
  Arguments were invalid, the caller buffer could not hold the frame, or the retained session failed or closed.
- `oxide_ios_reachability_start`, `oxide_ios_reachability_poll`, and `oxide_ios_reachability_close`
  Own an independent retained path monitor and copy its latest `OxideReachabilityStatus` snapshot.
- `oxide_host_net_set_reachability_callback`, `oxide_host_net_start_reachability`, and `oxide_host_net_stop_reachability`
  Install the Rust platform callback and own the process-level reachability monitor used by `IosReachability`.

## Logic narrative
- The caller constructs explicit connection and retry records, then `oxide_ios_quic_connect` validates both records before allocating native session state.
- `force_tcp_tls` is read only from the caller's `OxideQuicConfig`; there is no process-global environment override that can silently replace application policy.
- The implementation initializes `out_len` to zero, attempts the existing zero-timeout receive path, and copies one frame when available.
- A queued frame takes precedence over terminal state so valid frames accepted before EOF can drain in order.
- When no frame is available, the implementation reads the serialized closed state to distinguish idle from terminal.
- An undersized buffer closes the session and returns terminal rather than discarding a frame while reporting idle.

## Preconditions and postconditions
- `oxide_ios_quic_connect` requires an endpoint with an explicit port, non-null connection/retry-policy pointers, and at least one allowed attempt; TLS policy remains optional.
- `handle`, `buffer`, and `out_len` must be non-null.
- `buffer_len` must accommodate every accepted wire frame; the bridge's maximum frame size is 16 MiB.
- `out_len` is zero for idle and terminal, and contains the copied frame length only for frame status.
- The call never waits for future network input.

## Edge cases and failure modes
- Missing connection/retry policy, a zero attempt budget, or an endpoint without an explicit port fails connection creation without substituting hidden defaults.
- Invalid pointers return terminal; `out_len` is cleared first when its pointer is valid.
- A frame larger than `buffer_len` closes the retained session and returns terminal.
- A terminal session with already queued frames returns those frames before its final terminal result.

## Concurrency and memory behavior
- Frame removal and terminal-state reads serialize on the connection queue through the Objective-C implementation.
- The API allocates no native buffer; it copies directly into caller-owned storage.

## Performance notes
- Idle polling performs a nonblocking semaphore check and one serialized state read.
- Frame polling performs one bounded copy already required by the C/Rust boundary.

## Feature flags and cfgs
- The symbol is compiled with the platform-iOS native Network.framework bridge on Apple targets.

## Testing and benchmarks
- `tests/network_bridge_tests.rs` freezes the complete Oxide-owned ABI, rejects product-prefixed names and hidden connection-policy defaults, and guards nonblocking receive plus terminal state transitions.
- Device and simulator Objective-C syntax builds verify the header and implementation together.

## Example
```c
size_t frame_length = 0;
int32_t status = oxide_ios_quic_poll_recv(handle, buffer, capacity,
                                             &frame_length);
if (status == OXIDE_IOS_QUIC_POLL_FRAME)
{
   consume_frame(buffer, frame_length);
}
```

## Changelog
- 2026-07-11: hard-cut the complete transport/reachability ABI to Oxide-owned names, centralized every public layout and symbol in this header, and made connection/retry policy caller-owned.
- 2026-07-10: added the tri-state nonblocking retained-session receive ABI.
