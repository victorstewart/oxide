# `oxide-apple-comparison-controller` `tests/trace_tests.rs`

## Intention and purpose

These tests freeze the fail-closed macOS trace correlation contract without launching a comparator application.

## Relation to the rest of the code

Fixtures use the same mnemonic-driven `trace-query-result` shape exported by `xctrace`, including nested process values and cross-row references.

## Entry points list

- `correlates_generation_markers_through_exact_process_update_and_swap_id()` covers the complete generation-minus-one and swap join.
- `selects_a_unique_update_that_contains_visual_generation()` covers the containing-update preference.
- `preserves_missing_per_generation_display_opportunity_and_rejects_invalid_generation_or_swap()` covers explicit `null` display evidence and fail-closed generation/swap validation.
- `records_an_intervening_visual_generation_as_uncorrelated_instead_of_dropping_it()` covers superseded/coalesced evidence.
- `rejects_malformed_xml_references_and_marker_messages()` covers malformed exported evidence.

## Logic narrative

Each fixture supplies signposts, exact-process updates, and frame lifetimes independently. Assertions verify the selected marker times, generation edge, process, display, swap ID, interval bounds, candidate deltas, and uncorrelated reason.

## Preconditions and postconditions

Tests require no device, application, Instruments capture, or production code.

## Edge cases and failure modes

The suite rejects missing reference targets, malformed messages, generation zero, and duplicate frame-lifetime joins. A missing per-generation display opportunity remains visible as `None`.

## Concurrency and memory behavior

Fixtures are small and tests are independent.

## Performance notes

No benchmark timing is acquired. The test suite validates parser semantics only.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test trace_tests`.

## Examples

No additional flags are required.

## Changelog

- 2026-07-19: introduced focused trace-correlation coverage.
