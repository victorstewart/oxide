# `oxide-image-sampling-diagnostic`

## Intention and purpose

This non-publishable comparison crate isolates the macOS image-resampling investigation from every production Oxide crate and host. It decodes the frozen 4096×3072 PNG with the same Rust `png` normalization used by the comparison runtime, persists its canonical RGBA SHA-256, owns a fixed sampling matrix, and invokes the command-line AppKit/Metal reference helper.

## Relation to the rest of the code

- `oxide-image-sampling-diagnostic` reads the frozen `image.decode-zoom` source asset.
- `AppKitImageSamplingReference.swift` consumes the Rust-decoded BGRA bytes and the serialized matrix.
- The helper renders matching 256×192 regions through AppKit and a minimal Metal reproduction of Oxide's image texture/sampler/UV contract.
- The resulting `report.json` records exact RGB mismatch metrics for every pair.

No production host, scene, renderer crate, shader, or shipping dependency consumes this crate.

## Entry points

- `sampling_request() -> SamplingRequest`: returns the frozen four-ratio, three-phase diagnostic matrix.
- `decode_canonical_rgba(source_png: &[u8]) -> Result<(u32, u32, Vec<u8>), SamplingDiagnosticError>`: normalizes supported PNG color layouts to tightly packed RGBA8.
- `sha256(bytes: &[u8]) -> String`: produces lowercase artifact identities.
- `run(source_path: &Path, output_directory: &Path) -> Result<SamplingDiagnosticSummary, SamplingDiagnosticError>`: writes the request and Oxide-decoded source, executes the offscreen Swift helper, validates the report identity, and returns its persisted path.

## Logic narrative

The matrix uses full-image destination geometry but captures only a center 256×192 region. Its cases are physical 1:1, integer 2:1, the current 4096→1170 ratio, and the current 4096→2340 ratio. Each case runs at 0, 0.25, and 0.5 physical-pixel phase. The AppKit side evaluates `.none`, `.low`, and `.high`; the Metal side uses an sRGB texture, linear min/mag filtering, no mipmaps, clamp-to-edge, and the production image UV equation.

The call graph is:

- CLI `main`
  - `run`
    - Rust PNG decode and SHA-256
    - request/BGRA evidence write
    - command-line Swift helper
      - AppKit `NSImage.draw`
      - offscreen Metal linear reference
      - exact RGB reducer
    - report identity validation

## Preconditions and postconditions

The source must be the opaque 4096×3072 fixture. The helper must return schema version 1, the expected algorithm, matching encoded/decoded source identities, and exactly 36 pair rows. A successful call leaves `sampling-request.json`, `oxide-decoded-source.bgra`, and `report.json` in the requested directory.

## Edge cases and failure modes

Malformed PNG data, unsupported pixel layouts, unexpected dimensions or alpha, missing Apple command-line tools, absent Metal support, helper failure, incomplete reports, and identity mismatches fail with `SamplingDiagnosticError`. A non-exact comparison is evidence rather than an execution error and remains persisted.

## Concurrency and memory behavior

The probe is single-process and sequential. It allocates one canonical 50 MiB RGBA source, converts it in place to BGRA, and bounds each rendered sample to 49,152 pixels. It performs no work in a production frame path.

## Performance notes

This is correctness attribution, not a throughput benchmark. AppKit and Metal commands are serialized deliberately so the report is deterministic and resource usage stays bounded. No result is evidence of a production performance win.

## Feature flags and cfgs

There are no feature flags. Execution requires macOS because the helper imports AppKit and Metal; the Rust decoder and tests remain ordinary workspace code.

## Testing and benchmarks

`tests/lib_tests.rs` freezes the encoded and decoded source hashes, matrix ratios/phases, bounded ROI, pair count, and malformed-PNG failure. A complete manual probe uses:

```text
cargo run --locked -p oxide-image-sampling-diagnostic -- benchmarks/comparative/specs/v1/assets/image-decode-zoom-source-v1.png /private/tmp/oxide-image-sampling
```

## Examples

The command above persists a report without launching an application or creating a window.

## Changelog

- 2026-07-19: introduced the comparison-only decoded-pixel and AppKit/Metal sampling diagnostic.
