# `oxide-apple-comparison-controller::tests::time_profiler_tests`

## Intention and purpose

These deterministic tests prove the Time Profiler reducer without launching Instruments or consuming a GUI session.

## Relation to the rest of the code

Synthetic `time-profile` and `os-signpost` XML use the same schema mnemonics consumed by `reduce_macos_time_profiler_trace` and exercise exact-PID phase attribution.

## Entry points list

The test target has no public entry points.

## Logic narrative

One fixture includes two measured phases, main and worker samples, repeated stack identities, and an unrelated process. Assertions freeze weighted totals, main-thread attribution, scenario/phase identities, and deterministic top-stack ordering. Mutations prove missing PID, incomplete boundaries, zero weights, and empty measured intervals fail.

## Preconditions and postconditions

Fixtures are self-contained UTF-8 XML. Passing tests guarantee the public reducer accepts the valid contract and rejects each declared integrity failure.

## Edge cases and failure modes

No filesystem, process, timing, or network state is used.

## Concurrency and memory behavior

Tests are synchronous and bounded by the fixture strings.

## Performance notes

This is correctness coverage, not a performance population.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test time_profiler_tests`.

## Examples

The fixture is the minimal supported xctrace XML shape for reducer tests.

## Changelog

- 2026-07-21: added exact-PID Time Profiler reducer coverage.
