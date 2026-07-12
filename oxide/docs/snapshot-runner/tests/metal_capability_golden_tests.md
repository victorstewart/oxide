# oxide-snapshot-runner tests `metal_capability_golden_tests.rs`

## Intention and purpose

These macOS tests freeze the capability-specific Metal single-sample, 4x MSAA, and BGRA10_XR EDR paths required by C03.

## Relation to the rest of the code

They render the same asymmetric rounded rectangle through production `MetalRenderer` configurations and use snapshot-feature color-target readback.

## Entry points list

- `metal_single_sample_and_msaa4x_have_exact_capability_goldens`
- `metal_edr_bgra10xr_has_exact_raw_field_golden`

## Logic narrative

The MSAA test requires a real 4x-capable device, freezes independent exact images, and proves multisampling changes fractional edge coverage. The EDR test requires BGRA10_XR, decodes each 64-bit pixel's four padded XR10 components using Apple's `(value - 384) / 510` mapping, and therefore protects the extended-range target path without pretending it is an sRGB screenshot.

## Preconditions and postconditions

The macOS Metal device must support 4x MSAA and BGRA10_XR. All three capability images match exactly.

## Edge cases and failure modes

Capability fallback, wrong target format, resolve failure, packed-channel drift, or systematic edge changes fail explicitly.

## Concurrency and memory behavior

Each test owns its renderer. Snapshot-only readback waits for a dedicated blit after the submitted frame.

## Performance notes

These are correctness checks outside production builds and frame paths.

## Feature flags and cfgs

The integration test is macOS-only and uses `renderer-metal/snapshot-tests` through snapshot-runner's dependency configuration.

## Testing and benchmarks

Run `cargo test --locked -p oxide-snapshot-runner --test metal_capability_golden_tests`.

## Examples

Set `UPDATE_GOLDENS=1` only to review an intentional capability-image update.

## Changelog

- 2026-07-12: added exact single-sample, 4x MSAA, and packed BGRA10_XR EDR goldens.
