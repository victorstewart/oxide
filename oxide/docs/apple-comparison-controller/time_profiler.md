# `oxide-apple-comparison-controller::time_profiler`

## Intention and purpose

This comparison-only module reduces an exact-process macOS Time Profiler acquisition into measured scenario-phase CPU attribution. It keeps invasive sampling out of the primary Animation Hitches presentation pass and out of every Oxide production artifact.

## Relation to the rest of the code

The host controller records the `Time Profiler` template with `os_signpost` attached to the launched comparator PID, exports the `time-profile` and `os-signpost` tables, and calls `reduce_macos_time_profiler_trace`. The reducer joins samples to the `ScenarioBegin`/`ScenarioEnd` and measured `PhaseBegin`/`PhaseEnd` events emitted by `BenchmarkCampaignExecutor`.

Call flow:

- `run_foreground_session`
  - exact-PID `xctrace record --template Time Profiler`
  - export TOC, `time-profile`, and `os-signpost`
  - `reduce_macos_time_profiler_trace`
  - persist and resume-revalidate the typed artifact

## Entry points list

- `reduce_macos_time_profiler_trace(time_profile_xml: &str, signposts_xml: &str, exact_pid: u32) -> Result<MacOsTimeProfilerArtifact>` validates exact-PID samples and measured boundaries, then returns weighted per-phase attribution.
- `MacOsTimeProfilerArtifact`, `MacOsTimeProfilerPhase`, and `MacOsTimeProfilerStack` are the persisted campaign evidence types.

## Logic narrative

The reducer selects one formatted process identity ending in the exact controller PID. It pairs unique ordered scenario and phase boundaries, rejects incomplete or overlapping measured intervals, and assigns samples with half-open `[begin, end)` semantics. Each measured phase records total and main-thread sample counts and weights. Stack strings are grouped deterministically, sorted by descending weight and sample count then lexical identity, and capped at 20 rows per phase.

## Preconditions and postconditions

The PID is nonzero. Both XML exports contain their declared schema. Every sample has time, process, thread, positive weight, and stack attribution. Every measured phase is wholly inside one scenario and contains at least one exact-PID sample. Success returns nonempty, ordered phase evidence bound to one exact formatted process.

## Edge cases and failure modes

Missing schemas, malformed references, another PID, ambiguous formatted identities, unmatched/duplicate/reversed boundaries, non-Boolean measured flags, overlapping phases, zero-weight rows, empty measured phases, and arithmetic overflow fail closed.

## Concurrency and memory behavior

Reduction is synchronous after recording has finalized. XML rows and bounded stack maps are owned by the controller process; no measured application thread executes reducer work.

## Performance notes

Time Profiler is intentionally invasive descriptive attribution. It never supplies the primary presentation estimator. Recording uses the shared 512 MiB bundle-plus-isolated-scratch fuse and deterministic scratch cleanup.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/time_profiler_tests.rs` covers exact-PID filtering, phase assignment, main-thread weights, deterministic top stacks, missing PID, incomplete boundaries, zero weights, and empty measured intervals.

## Examples

Call the reducer with the two XML exports and the PID captured in the controller receipt; persist the returned artifact only after it equals a second reduction during validation.

## Changelog

- 2026-07-21: added isolated exact-PID Time Profiler acquisition reduction.
