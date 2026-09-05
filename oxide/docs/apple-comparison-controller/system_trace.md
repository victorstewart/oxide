# `oxide-apple-comparison-controller::system_trace`

## Intention and purpose

This harness-only module acquires and reduces the standalone macOS Instruments System Trace attribution pass. It reports main-thread running time, runnable wait time, context switches, and wakeups only inside the comparator's measured `PhaseBegin`/`PhaseEnd` intervals. It never enters an Oxide production binary.

## Relation to the rest of the code

The generic Apple campaign selects this path only when a pass declares `collector=system-trace`. The controller already knows the exact launched comparator PID and starts one native arm64 `xctrace` process attached to that PID. The module exports Xcode 26's `os-signpost`, `thread-info`, `thread-state`, and `context-switch` schemas, identifies the unique main thread, reduces the measured intervals, and leaves the finalized raw trace beside its XML and JSON evidence.

Call graph:

- `run_foreground_session`
  - `start_system_trace`
    - `MacOsSystemTraceCollector::start`
    - `MacOsSystemTraceCollector::wait_for_started`
  - comparator emits measured phase boundaries
  - `MacOsSystemTraceCollector::finish`
    - finalize the exact standalone System Trace
    - export TOC and four evidence tables with isolated temporary storage
    - `reduce_macos_system_trace`
    - durably persist `MacOsSystemTraceSummary`
  - `validate_session_files` recomputes and compares the summary during resume

## Entry points list

- `oxide_apple_comparison_controller::reduce_macos_system_trace(signposts_xml: &str, thread_info_xml: &str, thread_state_xml: &str, context_switch_xml: &str, pid: u32) -> Result<MacOsSystemTraceSummary>` validates Xcode 26 XML and reduces exact-PID, exact-main-thread measured phases.
- `oxide_apple_comparison_controller::MacOsSystemTraceSummary` is the durable per-session attribution artifact.
- `oxide_apple_comparison_controller::MacOsSystemTracePhaseSummary` contains one measured phase's scheduler metrics.
- `oxide_apple_comparison_controller::MACOS_SYSTEM_TRACE_AVAILABILITY` identifies successfully reduced evidence.

The collector owner and its path bundle are crate-private because only the controller may launch or finalize profiling processes.

## Logic narrative

The collector registers a Darwin trace-start waiter before spawning `/usr/bin/arch -arm64 xcrun xctrace record --template System Trace --attach PID`. It does not add another instrument and excludes the normal Animation Hitches and Time Profiler recorders for this pass. Record and export children receive controller-owned `TMPDIR`, `TMP`, and `TEMP` directories.

A 100 ms monitor counts the trace bundle and scratch tree together. Crossing 512 MiB, failing to measure the tree, timing out, or dropping the owner terminates the recorder process group so Instruments descendants cannot survive. Successful finalization requires a complete Instruments bundle. The raw trace remains; scratch storage is removed after all exports and reduction complete.

The reducer requires the exact Xcode 26 column layouts. `thread-info` must name exactly one main thread for the requested PID. Every measured boundary must come from that exact PID and thread. Begin/end keys are `(scenario, identifier)`; duplicate, missing, reversed, or overlapping phases fail. Main-thread state intervals must continuously cover each phase. Running and Runnable intervals are clipped to the phase, context switches are unique main-thread scheduler timestamps, and wakeups are transitions from a non-running/non-runnable state into Runnable.

## Preconditions and postconditions

The PID is nonzero and belongs to the already verified comparator executable. The plan declares `collector=system-trace`. Output and scratch paths must not exist. A successful result has at least one complete measured phase, exact PID/main-thread flags, nonempty scheduler evidence, a finalized raw trace, all six exported/derived files, and a resume-recomputable JSON artifact.

## Edge cases and failure modes

Unknown columns, missing schemas, empty tables, unresolved XML references, multiple main threads, mismatched nested PID/TID identity, malformed measured flags, unmatched boundaries, phase overlap, zero or overflowing state intervals, state gaps, absent main-thread context-switch evidence, trace growth, failed filesystem measurement, recorder exit, and incomplete finalization all fail closed. A non-System-Trace session containing any System Trace artifact is rejected.

## Concurrency and memory behavior

One monitor thread owns only atomic stop/fuse flags and immutable paths. The controller thread owns the `Child`, scratch guard, and artifact lifecycle. Acquire/release ordering publishes fuse failure before the controller proceeds. XML parsing allocates bounded row maps and phase vectors after capture; no parser or collector code runs inside the measured app.

## Performance notes

System Trace is intentionally a separate invasive attribution pass and cannot produce headline latency numbers. The standalone pass avoids combining profiler templates, while exact measured-phase clipping prevents setup, warmup, reset, and teardown scheduler work from contaminating attribution.

## Feature flags and cfgs

None. The acquisition command is macOS-specific at runtime; deterministic XML reduction remains testable anywhere the Rust crate builds.

## Testing and benchmarks

`tests/system_trace_tests.rs` uses frozen XML fixtures to verify exact-PID/main-thread reduction and expected running, runnable, context-switch, and wakeup totals. It also rejects missing/duplicate boundaries, identity mismatches, unsupported schemas, empty evidence, and incomplete state coverage. `tests/lib_tests.rs` freezes collector isolation, exact attachment, process-group teardown, storage fuse, required exports, controller support, and temp-environment ownership.

## Examples

The controller invokes this module from a generic Apple release plan. Offline tools can call `reduce_macos_system_trace` with the four exported XML documents and the controller receipt PID, then compare the returned artifact to the persisted session JSON.

## Changelog

- 2026-07-21: added standalone exact-PID System Trace acquisition, Xcode 26 scheduler reduction, strict resume validation, descendant teardown, and a 512 MiB bundle-plus-scratch fuse.
