# oxide-benchmark-spec::macos_scale

## Intention and purpose

`macos_scale` freezes the benchmark-only 1×/2× transformation for every macOS release scenario. It prevents a qualification run from labeling a second identical process run as “2×” without owning twice the declared semantic data or operation work.

## Relation to the rest of the code

- Comparative-spec overlay JSON supplies one content-addressed fixture identity and cardinality contract.
- The Apple comparison executor loads the same overlay for the AppKit and Oxide sides.
- `oxide-test-scenes` and the AppKit reference each retain an isolated second shard at 2× while presenting only the primary shard.
- `comparison-controller` admits profiler evidence only when its runtime attestation matches the overlay hash and effective cardinality.

Call graph:

- committed scale overlay
  - `validate_macos_comparator_scale_overlay`
  - comparator adapter configuration
  - primary shard plus optional isolated second shard
  - content-addressed runtime attestation

## Entry points list

- `oxide_benchmark_spec::macos_comparator_scale_contract(scenario_id: &str) -> anyhow::Result<(MacOsComparatorScaleDimension, MacOsComparatorScaleTransform, u64)>` returns the frozen dimension, transform, and 1× cardinality for one release scenario.
- `oxide_benchmark_spec::validate_macos_comparator_scale_overlay(overlay: &MacOsComparatorScaleOverlay, scenario_id: &str, fixture: &ArtifactIdentity) -> anyhow::Result<()>` proves scenario, fixture, transform, and exact 1×/2× arithmetic.
- `oxide_benchmark_spec::canonical_macos_comparator_scale_overlay_json(overlay: &MacOsComparatorScaleOverlay) -> anyhow::Result<Vec<u8>>` emits canonical pretty JSON plus a trailing newline.
- `oxide_benchmark_spec::canonical_macos_comparator_runtime_attestation_json(attestation: &MacOsComparatorRuntimeAttestation) -> anyhow::Result<Vec<u8>>` emits the matching runtime proof encoding.
- `MacOsComparatorScaleOverlay`, `MacOsComparatorRuntimeAttestation`, `MacOsComparatorScale`, `MacOsComparatorScaleDimension`, `MacOsComparatorScaleTransform`, and `MacOsComparatorSide` are the shared Rust/Swift wire contract.

## Logic narrative

Each scenario has one immutable base cardinality. Dataset scenarios select `namespaced_dataset_shards`: 1× owns one fixture-derived shard and 2× owns two independently mutable shards, with the second shard logically namespaced so it cannot collide with the visible primary state. Fixed scenes select `isolated_operation_shadow`: each canonical state transition is applied to a second isolated model at 2×. Only the primary model is presented, so screenshot, state, accessibility, artifact, and release gates remain unchanged. Validation calculates effective cardinality with checked multiplication and rejects any overlay that changes the frozen fixture or transformation.

## Preconditions and postconditions

The scenario and fixture identities must already be content-addressed. Success proves that effective cardinality equals exactly one or two times the canonical base and that the transform belongs to that scenario family.

## Edge cases and failure modes

Unknown scenarios, schema drift, fixture substitution, a swapped dimension or transform, zero/incorrect cardinality, and multiplication overflow fail closed.

## Concurrency and memory behavior

The contract is immutable owned data with no locks or global state. Two-x dataset adapters intentionally retain a second fixture-derived model for the bounded profiler session; teardown must release it with the visible model.

## Performance notes

Validation is setup-only. The second shard is real benchmark work and memory, but it never changes production crates or production renderer paths.

## Feature flags and cfgs

There are no features or target cfg differences.

## Testing and benchmarks

`tests/macos_scale_tests.rs` covers all 13 release scenarios at 1× and 2×, exact canonical encoding, and fail-closed transform/cardinality/fixture handling. The headed qualification campaign supplies Time Profiler evidence; these unit tests do not claim performance results.

## Examples

Load one committed overlay through the benchmark spec loader, validate it against the selected scenario fixture, configure the comparison adapter before `prepare`, then persist the adapter attestation after the single scenario run.

## Changelog

- 2026-07-21: introduced the shared 13-scenario macOS 1×/2× transformation and attestation contract.
