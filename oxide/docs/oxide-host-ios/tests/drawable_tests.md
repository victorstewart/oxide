# oxide-host-ios tests `drawable_tests.rs`

## Intention and purpose

These source-contract tests keep iOS drawable acquisition, frame preparation, retained scratch, damage handoff, and memory-pressure behavior aligned with the Rust-owned rendering lifecycle.

## Relation to the rest of the code

The tests inspect `oxide-host-ios/src/lib.rs` and the Objective-C app shell without launching UIKit. Runtime Metal behavior is covered separately by renderer and device tests.

## Entry points list

- `memory_warnings_purge_effect_targets_and_request_a_frame()` requires critical pressure to purge effect targets and prepared chunks before marking the frame dirty.
- Other tests freeze late drawable acquisition, prepared-frame cancellation, native coalescing scratch, and reusable damage storage.

## Logic narrative

Each test isolates the relevant source section and requires the exact host-to-renderer call sequence. The memory-warning gate keeps cache release in Rust and guarantees the next ordinary frame reconstructs visible resources instead of adding UIKit-owned recovery state.

## Preconditions and postconditions

The source markers used to isolate handlers must remain present. Passing proves required calls are wired; it does not substitute for runtime Metal validation.

## Edge cases and failure modes

A renamed or removed purge call fails explicitly. Missing handler boundaries fail before substring assertions can pass accidentally against unrelated code.

## Concurrency and memory behavior

Tests only read compile-time source strings. The production handler runs while holding the app-state mutex and releases renderer cache ownership synchronously before scheduling another frame.

## Performance notes

Prepared-cache purge is memory-pressure-only and adds no ordinary frame work.

## Feature flags and cfgs

The tests run on the macOS development host and do not require an iOS simulator.

## Testing and benchmarks

Run `cargo test --locked -p oxide-host-ios --test drawable_tests`.

## Examples

The required pressure sequence contains `renderer.purge_effect_targets();`, `renderer.purge_layer_cache_for_memory_warning();`, `renderer.purge_prepared_chunks();`, then `mark_frame_dirty(app);`.

## Changelog

- 2026-07-14: required the iOS memory-warning handler to purge retained layer storage alongside effect and prepared caches.
- 2026-07-13: required critical memory warnings to purge persistent prepared chunks.
