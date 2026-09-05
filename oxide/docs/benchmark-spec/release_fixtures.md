# oxide-benchmark-spec `release_fixtures`

## Intention and purpose

This module gives the five frozen release-candidate fixtures strict Rust types. Unknown fields are rejected so a comparator cannot silently add work that is absent from the shared contract.

## Relation to the rest of the code

- Candidate JSON in `benchmarks/comparative/specs/v1/fixtures` is the upstream source.
- `oxide-test-scenes::comparative` consumes these types for Oxide release-scene adapters.
- Native comparison adapters decode the same fields at their framework boundary.

## Entry points

The public structs are `GridFixture`, `EffectsFixture`, `MutationFixture`, `TextFixture`, `ResizeFixture`, and their nested value types. Serde deserialization is their only behavioral entry point.

## Logic narrative

Serde maps every declared JSON field into an owned value and rejects unknown fields. No fixture policy is inferred by the adapter.

## Preconditions and postconditions

Input must match schema version 1 field shapes. Successful decoding preserves all identities, counts, mutation formulas, text, viewport changes, and animation values.

## Edge cases and failure modes

Missing, mistyped, or extra fields fail decoding. Numeric overflow fails before an adapter is built.

## Concurrency and memory behavior

Types are owned and contain no synchronization. Decoding allocates only for fixture strings and vectors during scenario preparation, never per frame.

## Performance notes

These types are cold-path benchmark configuration. They add no production or frame-loop work.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/release_fixtures_tests.rs` decodes all five frozen fixtures and proves unknown work fails closed.

## Examples

```rust
let fixture: GridFixture = serde_json::from_slice(bytes)?;
```

## Changelog

- 2026-07-21: Added strict types for the five macOS release candidates.
