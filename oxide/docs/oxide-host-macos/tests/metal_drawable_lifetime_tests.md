# oxide-host-macos::tests::metal_drawable_lifetime_tests

## Intention and purpose
These tests lock the macOS Metal host's drawable-lifetime contract. The host must prepare the Rust/Oxide frame before asking `CAMetalLayer` for a drawable, acquire the drawable with timeout support, submit immediately after acquisition, and retry pending damage when drawable pressure skips a prepared frame.

## Relation to the rest of the code
- `src/macos/app.m` owns the AppKit `NSView` and `CAMetalLayer` draw loop.
- `src/lib.rs` owns the Rust prepared-frame state, pending damage, and renderer submission.
- `oxide-renderer-metal` consumes the prepared draw list and optional damage object during `begin_frame`, `encode_pass`, and `submit`.

Call flow:

- `MetalView::drawRect` -> `macos_app_prepare_frame`
- `MetalView::drawRect` -> `[CAMetalLayer nextDrawable]`
- `MetalView::drawRect` -> `macos_app_submit_prepared_frame_with_drawable`
- timeout path -> `macos_app_cancel_prepared_frame` -> retained pending damage for retry

## Entry points list
- `draw_rect_prepares_frame_before_acquiring_drawable()`
  Reads `src/macos/app.m` and verifies `macos_app_prepare_frame` appears before `nextDrawable`, with submit after drawable acquisition.
- `draw_rect_uses_timeout_capable_drawable_acquisition()`
  Reads `src/macos/app.m` and verifies `maximumDrawableCount`, `allowsNextDrawableTimeout`, and cancellation on nil drawable.
- `canceled_prepared_frame_retains_damage_for_retry()`
  Reads `src/lib.rs` and verifies prepared damage is merged, retained on cancel, and restored on submit failure.

## Logic narrative
The tests are static contract tests because drawable pressure is timing-sensitive and hard to reproduce deterministically in libtest. The source checks guard the ordering and state transitions that make the runtime path safe: prepare first, acquire late, cancel on timeout, and keep dirty regions alive for the next frame.

## Preconditions and postconditions
- The tests assume the macOS host keeps the FFI entry point names stable.
- Passing means the host does not acquire a drawable before building the frame and does not clear pending damage when a prepared frame is skipped.

## Edge cases and failure modes
- If `nextDrawable` is acquired before prepare, the first test fails because drawable lifetime would include CPU-side frame building.
- If timeout support is removed, the second test fails because drawable pressure could block indefinitely.
- If cancellation clears pending damage, the third test fails because a skipped frame could lose a required redraw.

## Concurrency and memory behavior
The tests do not create AppKit objects or threads. Production state remains protected by the host's process-global mutex, and retained damage reuses the existing `Vec<RectI>` allocation where possible by appending or moving rather than cloning.

## Performance notes
The production change avoids blocking and preserves damage without adding work to successful frame submissions. Retention only matters on timeout or submit failure; successful frames move the damage vector into the renderer as before.

## Feature flags and cfgs
These tests are ordinary macOS host integration tests and do not require iOS, simulator, device, or trace tooling.

## Testing and benchmarks
Run with:

```sh
cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-macos --test metal_drawable_lifetime_tests
```

## Examples
```rust
let source = include_str!("../src/macos/app.m");
let draw_rect = source.find("- (void)drawRect:").expect("drawRect");
let tail = &source[draw_rect..];
assert!(tail.find("macos_app_prepare_frame") < tail.find("nextDrawable"));
```

## Changelog
- 2026-06-01: Added prepared-frame damage-retention coverage for drawable timeout and submit failure retry paths.
