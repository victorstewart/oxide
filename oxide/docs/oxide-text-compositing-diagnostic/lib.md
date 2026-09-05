# `oxide-text-compositing-diagnostic`

## Intention and purpose

This non-publishable comparison crate isolates the remaining 20-point macOS title residue without changing Oxide production text, Metal, scenes, or the AppKit reference adapter. It freezes the `Navigation` and `Decode & Zoom` geometries, their pinned Noto Sans variable-font identity, and all 35 one-channel/one-LSB v28 signatures.

## Relation to the rest of the code

Rust owns the immutable request, invokes both command-line Swift helpers, and validates every persisted A8/RGB/coverage artifact against frozen SHA-256 identities. `TextCompositingReference.swift` shapes the same CoreText runs and evaluates direct glyph drawing, A8 mask-fill, and scalar CPU replay. `production_probe.rs` then uses the real `oxide-text` and `ui-core` label path to capture the production 1024² A8 page and exact lowered glyph instances. `MetalTextProbe.swift` replays those bytes through point and production-linear coverage passes plus the complete fixed-blend `BGRA8Unorm_sRGB` path.

No production or reference-adapter target depends on this crate.

## Entry points

- `diagnostic_request() -> DiagnosticRequest`: returns the two frozen title cases and 35 known signatures.
- `sha256(bytes: &[u8]) -> String`: returns a lowercase artifact identity.
- `run(font_path: &Path, output_directory: &Path) -> Result<DiagnosticSummary, DiagnosticError>`: validates the font, persists the request, runs the offscreen helper, and validates the report and output files.

## Logic narrative

The request pins a 1170×174 physical canvas at scale three, Noto Sans VF at 20 points with `wght=400` and `wdth=100`, background `[243,245,248]`, and text `[32,36,44]`. The helper creates one CoreText line per title, applies the same quantized x origin and centered baseline formulas as the comparison scene, and draws identical glyph IDs and positions through all three paths.

The direct path reproduces AppKit's float-linear screenshot context. The A8 path mirrors Oxide's alpha-only CoreGraphics context settings. Mask-fill keeps CoreGraphics responsible for compositing the captured mask. Metal CPU replay uses the same scalar coverage and linear source-over model before deterministic sRGB8 encoding. Every raw output and A8 plane is persisted and hashed; each known coordinate records its A8 value and all three RGB triplets beside the v28 native/Oxide expectations.

The frozen run found byte-identical direct and mask-fill outputs for both titles. Direct and mask-fill each reproduce 30 of the 35 AppKit signatures; the scalar replay reproduces 5 of the 35 Oxide signatures. Across the complete canvases, scalar replay differs from direct/mask-fill at 3 `Navigation` pixels and 9 `Decode & Zoom` pixels, always by one channel level.

The production Metal probe resolves the remainder exactly. Point- and linear-sampled coverage have identical full-canvas float identities, and CPU composites from both are byte-identical to the scalar replay. The complete fixed-function source-alpha blend into `BGRA8Unorm_sRGB` reproduces all 35 Oxide signatures. Earliest-stage attribution is therefore 5 scalar-A8 replay, 0 production atlas/geometry, 0 linear sampler, 30 fixed-blend/sRGB target, and 0 unexplained. The remaining 30 one-LSB differences are not caused by atlas rasterization, atlas coordinates, or filtering; they first appear at the fixed-function blend/render-target conversion boundary.

Frozen artifact identities are:

- `Navigation`: A8 `ffcfea7d4c7cd844a7e49b91d8ac1d4e18a83b10e3cdd4d6de65949937de4729`; direct and mask-fill `a5be0652c49f70f47ef7a79825f14645dc7cd4ed67d1dc0563fe56df61b9a2d9`; CPU replay `687c0f9288e176d816bc8869af668e9745dfe98d244e03bf8e06040180a09cb3`.
- `Decode & Zoom`: A8 `d4fc7a761b0c00e7268365402b0c5565d78a2bdbc30f9907f3d561f5ecb7b17c`; direct and mask-fill `f1eac519edd2fa4a9f521b03007274fcb849169f8e312b250010f133a65ecc3a`; CPU replay `0565f6e8a96eea2350639f53058383d1e75aeed8e81a94d94983e59d78af3d51`.

Production-Metal artifact identities are:

- `Navigation`: production atlas `c6c58c0e11544168432d58e9f67753323d7d887f0be4acf950d0776b0be7cddb`; point and linear coverage `d979e2cd81a80dabb7422ecb600d79071724fecfbc8cd13ed272de32201a308f`; point and linear CPU composite `687c0f9288e176d816bc8869af668e9745dfe98d244e03bf8e06040180a09cb3`; complete Metal `a7b6e99afd0048f0b022594015087ca706217e18b117b60bb188caa0997d8be7`.
- `Decode & Zoom`: production atlas `534a809a96b115e2a47e9d5c9b7481556fefd71dfb9d5d08ef5d35e73e93421e`; point and linear coverage `5e72219b1f64c47d9b2d137877f5360312644ddb93400ba947bc27c35d8c6243`; point and linear CPU composite `0565f6e8a96eea2350639f53058383d1e75aeed8e81a94d94983e59d78af3d51`; complete Metal `39d1420fa6302215975a13f206bf87a915707eb7cb1ebffffb17e4249efc639d`.

## Preconditions and postconditions

The input font must have SHA-256 `bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d`. macOS command-line Swift, CoreText, CoreGraphics, Metal, and linear sRGB must be available. Success produces two identity-checked reports, exactly two cases and 35 signatures, the CoreText/A8 outputs, the production atlas and instance manifests, two float coverage planes, two CPU composites, and one complete Metal output per case.

## Edge cases and failure modes

Wrong font bytes, malformed request/report JSON, unavailable Apple frameworks, unexpected artifact sizes, helper failure, incomplete signatures, a changed match count, or SHA mismatch fail closed. A path that does not match a v28 signature remains valid diagnostic evidence and is never treated as visual acceptance.

## Concurrency and memory behavior

Execution is sequential and offscreen. Each 1170×174 A8/RGB/float plane and each 1024² production atlas is bounded; there is no window, application object, display link, drawable, or production frame-loop work. The diagnostic uses short-lived Metal resources and waits only in its command-line process.

## Performance notes

This is attribution tooling, not a performance benchmark. It makes no production performance claim and cannot justify a renderer change without separate balanced A/B evidence.

## Feature flags and cfgs

There are no features. Request construction is portable Rust; complete helper execution requires macOS.

## Testing and benchmarks

```text
cargo test --locked -p oxide-text-compositing-diagnostic
cargo run --locked -p oxide-text-compositing-diagnostic -- \
  benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1/NotoSans-VF.ttf \
  /private/tmp/oxide-text-compositing
```

## Examples

The command above persists the bounded report without launching an app or window.

## Changelog

- 2026-07-19: introduced the isolated CoreText/A8/Metal-quantization title diagnostic.
- 2026-07-19: added exact production-atlas point/linear sampling and fixed-blend sRGB target attribution, resolving all 35 signatures.
