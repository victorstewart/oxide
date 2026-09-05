# Runnable Apple PR scenario tests

## Intention and purpose

`crates/benchmark-spec/tests/pr_scenario_tests.rs` proves that the committed three-scenario vertical slice and complete six-scenario Apple PR set are canonical, content-addressed, semantically exact, and runnable by a future adapter.

## Relation to the rest of the code

- `fixtures::load_pr_vertical_scenarios` and `load_apple_pr_scenarios` provide frozen manifest order.
- `pr_scenarios` validates generic artifact closure plus typed fixture semantics.
- `scenario::canonical_scenario_json` proves committed manifest bytes have canonical field order.
- The PNG decoder verifies every correctness golden uses the exact 1170x2532 physical viewport.

## Entry points list

This integration-test binary exports no public API. Cargo's harness invokes `committed_pr_vertical_slice_is_canonical_complete_and_runnable` and `committed_apple_pr_set_is_canonical_complete_and_runnable`.

## Logic narrative

Each test loads manifests in frozen order, validates every referenced fixture, asset, font, layout, trace, state, accessibility, and screenshot hash, then checks canonical manifest bytes. The vertical test freezes dashboard/feed/navigation headline sizes. The complete-set test additionally checks 24 startup cards and 24 KiB data, 5,000 chat messages and 64 avatars, exact image dimensions, every design-golden dimension, and the exact four pending headed-recapture checkpoint markers.

## Preconditions and postconditions

The Cargo workspace contains the committed benchmark-spec v1 tree. Success proves the artifact graph and semantic contracts are internally runnable; it does not claim an Oxide or native adapter has rendered them or passed device parity.

## Edge cases and failure modes

Missing or reordered manifests, stale hashes, malformed traces, incorrect phase timing, workload-size drift, noncanonical JSON, missing fonts/assets, and incorrect screenshot dimensions fail the tests.

## Concurrency and memory behavior

Tests perform read-only synchronous file I/O. PNG readers decode headers only, avoiding full screenshot buffers. Full fixture validation owns parsed startup/feed/chat values and releases them after each test.

## Performance notes

All work occurs outside acquisition. Validation is linear in artifact bytes and deliberately reads the deterministic image assets to prove their hashes.

## Feature flags and cfgs

No feature flag or target cfg changes these tests.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test pr_scenario_tests`. Runtime performance is measured by the separate comparison harness, not these correctness tests.

## Examples

The tests are executable examples of loading and validating a complete scenario set before acquisition.

## Changelog

- 2026-07-21: froze the exact typed pending-recapture marker set.
- 2026-07-18: added full startup/dashboard/feed/chat/navigation/image artifact, semantic, canonical-byte, and golden-dimension coverage.
- 2026-07-18: added the initial dashboard/feed/navigation runnable vertical-slice coverage.
