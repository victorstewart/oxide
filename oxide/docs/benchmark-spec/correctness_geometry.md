# oxide-benchmark-spec correctness_geometry

## Intention and purpose

`correctness_geometry.rs` defines the versioned geometry evidence captured by the macOS comparison applications at a correctness checkpoint. It prevents fixture-authored layout or reducer constants from standing in for the bounds that the running AppKit and Oxide implementations actually presented.

## Relation to the rest of the code

- AppKit captures its live view/text rectangles and Oxide captures the primitive and glyph-run rectangles in the encoded draw list, in addition to each live root.
- `comparison-controller::correctness` verifies each hash-bound artifact and uses its scale and root for visual reduction.
- `release_promotion` accepts only a valid paired runtime geometry envelope and content-addresses both sides into each promoted checkpoint.

Call flow:

- runtime adapter bounds
  - `BenchmarkCampaignExecutor`
  - `geometry.actual.json`
  - `decode_macos_correctness_geometry`
  - correctness reduction or release promotion

## Entry points list

- `oxide_benchmark_spec::decode_macos_correctness_geometry(bytes: &[u8]) -> anyhow::Result<MacOsCorrectnessGeometryEvidence>` decodes and validates one runtime artifact.
- `oxide_benchmark_spec::decode_macos_correctness_geometry_pair(bytes: &[u8]) -> anyhow::Result<MacOsCorrectnessGeometryPairEvidence>` validates one promoted AppKit/Oxide envelope.
- `oxide_benchmark_spec::validate_macos_correctness_geometry(geometry: &MacOsCorrectnessGeometryEvidence) -> anyhow::Result<()>` enforces the version, coordinate space, capture profile, scale, origin, and exact logical-to-physical grid.
- `oxide_benchmark_spec::MacOsCorrectnessGeometryEvidence` is the shared serialized evidence shape.
- `oxide_benchmark_spec::MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE` and `MACOS_CANONICAL_CORRECTNESS_SCALE` freeze the macOS correctness capture profile.

## Logic narrative

Decoding first parses the typed JSON. Validation then requires schema version one, logical-point coordinates, the frozen opaque-sRGB8 3x capture profile, exact zero position/size tolerances, an origin at zero, positive whole-point root dimensions, and at least one contiguous finite positive runtime node. Pair validation requires the AppKit view-tree and Oxide draw-list sources, matching roots, profile, scale, and tolerances. Node arrays are intentionally source-specific: pixel reduction remains the cross-framework visual gate until semantic role/ID rectangles are captured by both implementations.

## Preconditions and postconditions

Input is a complete JSON object emitted outside measured phases. Success proves the root can be used directly as the visual layout contract and that `canonical_scale` is the profile-owned value rather than a reducer default.

## Edge cases and failure modes

Unknown versions or profiles, scale drift, translated roots, non-finite or nonpositive dimensions, fractional logical dimensions, and a non-exact physical grid fail closed.

## Concurrency and memory behavior

Validation is synchronous, immutable, and lock-free. It allocates only during JSON decoding.

## Performance notes

The function runs once per correctness checkpoint, outside measured presentation phases. It does not change renderer or production hot paths.

## Feature flags and cfgs

There are no feature flags. The schema is macOS-specific even though deterministic parsing is portable.

## Testing and benchmarks

`tests/correctness_geometry_tests.rs` covers the canonical landscape profile and rejects profile, scale, origin, and pixel-grid drift. No performance benchmark is required for off-window evidence validation.

## Examples

Decode `geometry.actual.json`, then pass the original bytes and the decoded `canonical_scale` to `compare_calibrated_static_pngs`.

## Changelog

- 2026-07-21: added hash-bound live AppKit view/text and Oxide encoded primitive/glyph geometry with a frozen canonical capture profile and paired promotion envelope.
