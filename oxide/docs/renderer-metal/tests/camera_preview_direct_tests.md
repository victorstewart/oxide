# oxide-renderer-metal tests `camera_preview_direct_tests.rs`

## Intention and purpose

This integration test verifies the direct synthetic Metal camera-preview path, including its bounded renderer configuration and snapshot-visible work counters.

## Relation to the rest of the code

- Cargo test constructs `oxide_renderer_metal::MetalRenderer` with synthetic camera textures.
- The renderer consumes the same direct-preview implementation used by the explicit camera performance path.

## Entry points list

- `direct_camera_preview_path_draws_single_synthetic_camera_frame()` exercises one synthetic direct-preview frame.
- The remaining tests freeze direct-preview resource, draw, and configuration behavior under supported benchmark controls.

## Logic narrative

The helper derives from `MetalRendererConfig::default`, overrides the camera mode/source, and enables direct-preview-only construction. This keeps the explicit offscreen/perf frame depth while reducing each direct-preview ring slot to its dedicated small capacity.

## Preconditions and postconditions

- A Metal device must be available for a rendered assertion; unsupported hosts may skip through the helper's `NoDevice` branch.
- Passing tests preserve the synthetic source and direct-preview-only mode.

## Edge cases and failure modes

- Unexpected initialization errors fail immediately.
- Environment overrides are restored after each scoped test.

## Concurrency and memory behavior

Tests serialize environment mutation and construct renderer resources outside production frame loops.

## Performance notes

The direct-preview ring begins at 4 KiB per slot and grows only on demonstrated demand. These tests are correctness contracts, not official device performance evidence.

## Feature flags and cfgs

Metal rendering behavior follows the crate's platform configuration; snapshot-only helpers remain feature-gated.

## Testing and benchmarks

Run `cargo test --locked -p oxide-renderer-metal --test camera_preview_direct_tests`.

## Examples

Use `MetalRendererConfig { direct_preview_only: true, ..MetalRendererConfig::default() }` for the explicit synthetic benchmark path.

## Changelog

- 2026-07-13: preserved explicit offscreen/perf depth after adding configurable frame-resource depth.
