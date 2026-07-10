# platform-ios `src/ios/network.h`

## Intention and purpose
- Publish the narrow C ABI needed to poll a retained iOS Network.framework session without blocking or conflating idle state with terminal state.

## Relation to the rest of the code
- `src/ios/network.m` implements the declared symbol and owns the opaque `NametagQuicHandle`.
- Downstream Rust FFI declares the same fixed-width return value and maps it into frame, idle, or terminal transport outcomes.
- `build.rs` watches this header so ABI edits rebuild the native bridge.

## Entry points list
- `int32_t nametag_ios_quic_poll_recv(NametagQuicHandle handle, uint8_t *buffer, size_t buffer_len, size_t *out_len)`
  Polls once without blocking, copies at most one complete queued frame, and returns `NAMETAG_IOS_QUIC_POLL_FRAME`, `NAMETAG_IOS_QUIC_POLL_IDLE`, or `NAMETAG_IOS_QUIC_POLL_TERMINAL`.
- `NAMETAG_IOS_QUIC_POLL_FRAME = 1`
  One complete frame was copied and `out_len` is its byte length.
- `NAMETAG_IOS_QUIC_POLL_IDLE = 0`
  No frame was queued at the polling instant and the retained session is not terminal.
- `NAMETAG_IOS_QUIC_POLL_TERMINAL = -1`
  Arguments were invalid, the caller buffer could not hold the frame, or the retained session failed or closed.

## Logic narrative
- The implementation initializes `out_len` to zero, attempts the existing zero-timeout receive path, and copies one frame when available.
- A queued frame takes precedence over terminal state so valid frames accepted before EOF can drain in order.
- When no frame is available, the implementation reads the serialized closed state to distinguish idle from terminal.
- An undersized buffer closes the session and returns terminal rather than discarding a frame while reporting idle.

## Preconditions and postconditions
- `handle`, `buffer`, and `out_len` must be non-null.
- `buffer_len` must accommodate every accepted wire frame; the bridge's maximum frame size is 16 MiB.
- `out_len` is zero for idle and terminal, and contains the copied frame length only for frame status.
- The call never waits for future network input.

## Edge cases and failure modes
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
- `tests/network_bridge_tests.rs` freezes the symbol, return values, nonblocking receive call, output-length rules, and terminal state transitions.
- Device and simulator Objective-C syntax builds verify the header and implementation together.

## Example
```c
size_t frame_length = 0;
int32_t status = nametag_ios_quic_poll_recv(handle, buffer, capacity,
                                             &frame_length);
if (status == NAMETAG_IOS_QUIC_POLL_FRAME)
{
   consume_frame(buffer, frame_length);
}
```

## Changelog
- 2026-07-10: added the tri-state nonblocking retained-session receive ABI.
