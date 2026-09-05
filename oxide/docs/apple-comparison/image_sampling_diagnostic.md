# AppKit image sampling reference helper

## Intention and purpose

`AppKitImageSamplingReference.swift` is a command-line, comparison-only renderer used to distinguish decoded-pixel, interpolation-kernel, and physical-pixel-phase differences without launching an AppKit application.

## Relation to the rest of the code

The Rust diagnostic supplies canonical Oxide BGRA bytes and an immutable request. The helper decodes the same PNG with AppKit, hashes canonical packed RGBA, renders `.none`, `.low`, and `.high` interpolation into the same linear-sRGB float capture path as the AppKit comparison adapter, and canonicalizes through BGRA8 sRGB Metal output. Its Metal reference independently reproduces the relevant production texture, sampler, UV clamp, and output format.

## Entry points

- Top-level `run()`: validates six command-line values and writes the exact JSON report.
- `MetalLinearReference.render`: renders one bounded Metal sample.
- `CanonicalSRGBEncoder.encode`: converts AppKit's linear float context through the canonical sRGB attachment.
- `renderAppKit`: draws one `NSImage` sample at a selected interpolation quality.
- `canonicalDecodedRGBA`: canonicalizes AppKit's packed decoded pixels for a cross-decoder hash.
- `difference`: compares opaque BGRA outputs over RGB channels.

## Logic narrative

For every ratio and phase, Metal renders once and AppKit renders three interpolation variants. The helper records output hashes, differing pixels/channels, maximum delta, bounds, and a complete delta histogram. Source pixels and the output ROI are persistent within the process; render targets are bounded and sequential.

## Preconditions and postconditions

The request must be schema version 1 with a 4096×3072 source, a positive ROI, phases `[0, 250000, 500000]`, and four cases. The Oxide BGRA file must be exactly 50,331,648 bytes. Success writes 36 pair rows atomically.

## Edge cases and failure modes

Unavailable AppKit decoding, unsupported packed pixels, Metal compilation/resource failures, command-buffer failures, source identity drift, or malformed requests terminate with a concise stderr error and nonzero status.

## Concurrency and memory behavior

The helper is sequential. `waitUntilCompleted` is intentional because this is an offline correctness probe, never a production frame loop. One source texture and one canonical encoder are retained; sample outputs are 256×192.

## Performance notes

The helper's Metal code is a faithful sampling reproduction, not the production renderer. The report explicitly preserves this gap so no sampling result can be presented as a production performance measurement.

## Feature flags and cfgs

The helper requires macOS AppKit, CryptoKit, and Metal frameworks.

## Testing and benchmarks

Use `xcrun swiftc -typecheck` for compilation and the Rust CLI for a complete offscreen probe.

## Examples

The helper is normally invoked by the Rust crate rather than directly.

## Changelog

- 2026-07-19: introduced canonical decoded hashes and the four-ratio, three-phase, three-quality matrix.
