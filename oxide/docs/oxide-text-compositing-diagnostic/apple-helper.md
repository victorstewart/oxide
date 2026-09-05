# `TextCompositingReference.swift`

## Intention and purpose

The Swift helper supplies the Apple-owned CoreText and CoreGraphics portions of the bounded experiment without modifying or launching the comparison application.

## Relation to the rest of the code

It consumes the Rust request and font, renders direct float-linear RGB and A8 mask-fill references, runs the CPU Metal quantizer, and writes raw planes plus the exact JSON attribution consumed by Rust.

## Entry points

- Top-level `execute()`: validates arguments and identities, prepares both title runs, executes all three paths, persists artifacts, and writes `report.json`.

## Logic narrative

CoreText creates the pinned lines and glyph positions once per title. Direct rendering mirrors the flipped scale-three AppKit capture context. A8 rendering mirrors Oxide's alpha-only antialiasing and subpixel settings. Mask-fill uses the same A8 bytes through CoreGraphics. CPU Metal replay applies A8/255 coverage to linear source and destination colors before sRGB8 quantization. Exact reducers and SHA-256 identities make every result auditable.

## Preconditions and postconditions

Arguments are font path, request path, output directory, and report path. Successful completion writes all requested raw artifacts and one sorted-key report.

## Edge cases and failure modes

Invalid dimensions, colors, variation tags, font identity, coordinates, context creation, or file operations fail with a nonzero status.

## Concurrency and memory behavior

The helper is single-threaded, offscreen, and bounded to the two 1170×174 titles.

## Performance notes

The helper is an attribution oracle, not a benchmark.

## Feature flags and cfgs

It requires macOS CoreText, CoreGraphics, CryptoKit, and Foundation.

## Testing and benchmarks

Rust integration tests invoke the helper and validate all artifact identities.

## Examples

Use the crate CLI rather than invoking this helper directly.

## Changelog

- 2026-07-19: added direct RGB, A8 mask-fill, and CPU Metal quantization paths.
