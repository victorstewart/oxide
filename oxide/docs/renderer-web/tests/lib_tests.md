# renderer-web::tests::lib_tests

## Intention and purpose

These tests verify the renderer-web behavior that can be exercised on native test targets without a browser DOM. They exist to keep shared conversion logic and the native unsupported stub deterministic.

## Relation to the rest of the code

The tests import `oxide_renderer_web` public helpers and `WebRenderer`. Native CI can run them even though the real renderer implementation is compiled only for wasm32.

Call flow:

- cargo test
- `renderer-web/tests/lib_tests.rs`
- public helper functions or native `WebRenderer`
- `oxide_renderer_api::Renderer` trait methods

## Entry points list

- `color_conversion_clamps_channels()`: verifies CSS color conversion and packed color cache keys.
- `sanitize_scale_rejects_invalid_values()`: verifies invalid scale fallback.
- `native_stub_tracks_frame_shape_and_reports_unsupported_submit()`: verifies native frame counters and unsupported submit behavior.

## Logic narrative

The tests intentionally avoid browser APIs. Color tests clamp overrange and underrange values. Scale tests cover valid, zero, and NaN values. The native stub test starts a frame with damage, encodes an empty draw list, inspects counters, and checks that submitting on a non-wasm target returns `RenderError::Unsupported`.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests require only the native Rust test runner. They maintain the invariant that native builds can compile and test the web crate without pretending to render.

## Edge cases and failure modes

NaN scale collapses to `1.0`. Overrange color channels clamp to byte limits. Native submission must not succeed because it would mask missing browser execution.

## Concurrency and memory behavior

The tests are single-threaded and allocate only small strings/vectors.

## Performance notes

These are correctness tests, not performance cases.

## Feature flags and cfgs

They run on native targets against the non-wasm `WebRenderer` stub.

## Testing and benchmarks

Run with `cargo test -p oxide-renderer-web --tests`.

## Examples

```rust
pub fn scale() -> f32
{
   oxide_renderer_web::sanitize_scale(0.0)
}
```

## Changelog

- Added initial native coverage for the web renderer support code.
