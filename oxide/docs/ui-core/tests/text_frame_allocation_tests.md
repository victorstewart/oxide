# oxide-ui-core tests `text_frame_allocation_tests.rs`

## Intention and purpose

Prove that C43 frame-scoped text preparation adds no heap allocation or reallocation to a warmed 1,000-label frame and performs no shaping, rasterization, or atlas upload work.

## Relation to the rest of the code

- Exercises public `oxide_ui_core::elements::TextCtx` frame and profiling APIs.
- Uses `oxide-wasm-alloc-counter` with the system allocator to observe the same label-encoding path used by the router.
- Protects the warm CPU guardrail recorded by `cpu.architecture.text.warm_labels_1000`.

## Entry points list

- `warm_thousand_label_frame_is_allocation_free()`
  Warms text/layout/glyph caches and draw-list capacities, measures one identical frame, and asserts zero allocation, reallocation, shaping, rasterization, and upload deltas.

## Logic narrative

The test loads the fixed Asap font, creates 1,000 stable labels, and encodes two complete frames so layout, glyph, atlas, and vector capacities are warm. It clears the draw builder without dropping capacity, snapshots allocator totals, encodes a third identical frame through `begin_frame` and `finish_frame`, then compares allocator and text-counter deltas.

## Preconditions and postconditions

- The bundled Asap fixture must remain present.
- The counting allocator must be installed before any measured allocation.
- A passing test proves only the warmed steady state; cold cache population is intentionally covered elsewhere.

## Edge cases and failure modes

- Any hidden profiler allocation, draw-list growth, shape miss, raster miss, or atlas publication fails independently.
- Uploader calls are deterministic and perform no hidden allocation.

## Concurrency and memory behavior

The test is single-threaded. It owns one `TextCtx`, uploader, and draw builder, and reuses their capacities across measured frames.

## Performance notes

This is an exact allocation-count contract, not a duration benchmark. The paired CPU and Metal cases provide latency distributions.

## Feature flags and cfgs

No feature-specific behavior.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-ui-core --test text_frame_allocation_tests`.

## Examples

```rust
cargo test --locked -p oxide-ui-core --test text_frame_allocation_tests
```

## Changelog

- 2026-07-14: added the C43 warmed 1,000-label allocation and zero-work contract.
