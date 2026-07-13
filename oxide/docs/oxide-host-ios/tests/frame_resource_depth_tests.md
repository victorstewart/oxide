# oxide-host-ios tests `frame_resource_depth_tests.rs`

## Intention and purpose

This source contract keeps the product iOS host on the normal visible Metal frame-resource mode.

## Relation to the rest of the code

The test inspects `oxide-host-ios/src/lib.rs`, which constructs `MetalRenderer` before scene/router startup.

## Entry points list

- `ios_product_renderer_selects_visible_frame_resource_depth()` requires the visible-host configuration and configured renderer initializer.

## Logic narrative

The host keeps camera mode/source overrides while inheriting the renderer's three-slot visible depth. It does not infer command-buffer lifetime from UIKit drawable count.

## Preconditions and postconditions

The source must retain the exact visible configuration call. Passing proves initialization wiring, not device timing.

## Edge cases and failure modes

A regression to offscreen/default construction fails before runtime/device testing.

## Concurrency and memory behavior

The test performs compile-time string inspection only.

## Performance notes

Visible startup allocates three dynamic slots rather than eight.

## Feature flags and cfgs

The source test runs on host test targets without launching UIKit.

## Testing and benchmarks

Run `cargo test --locked -p oxide-host-ios --test frame_resource_depth_tests`.

## Examples

The product initializer uses `..metal::MetalRendererConfig::visible_host()` after its camera-specific fields.

## Changelog

- 2026-07-13: added visible frame-resource construction coverage.
