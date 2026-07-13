# oxide-host-macos tests `frame_resource_depth_tests.rs`

## Intention and purpose

This source contract keeps the product macOS host on the normal visible Metal frame-resource mode.

## Relation to the rest of the code

The test inspects `oxide-host-macos/src/lib.rs`, which constructs the renderer used by AppKit frames.

## Entry points list

- `macos_product_renderer_selects_visible_frame_resource_depth()` requires configured visible construction and rejects default/offscreen construction.

## Logic narrative

The AppKit host selects three slots explicitly while the renderer continues to use actual command-buffer completion for safe reuse.

## Preconditions and postconditions

Passing proves host wiring remains explicit. Runtime Metal tests separately prove saturation and recovery.

## Edge cases and failure modes

Any return to `new_default()` fails because default preserves the deeper offscreen/perf mode.

## Concurrency and memory behavior

The test performs compile-time string inspection only.

## Performance notes

Visible startup allocates three dynamic slots rather than eight.

## Feature flags and cfgs

No platform runtime is required for this source contract.

## Testing and benchmarks

Run `cargo test --locked -p oxide-host-macos --test frame_resource_depth_tests`.

## Examples

The host calls `MetalRenderer::new_with_config(MetalRendererConfig::visible_host())`.

## Changelog

- 2026-07-13: added visible frame-resource construction coverage.
