# `xtask::xctrace_record`

## Intention and purpose

This module owns every `xctrace record` child launched by `xtask`. It prevents Instruments from writing uncontrolled temporary `.ktrace` data into the account-wide macOS temporary directory and enforces a 512 MiB hard limit across the retained `.trace` bundle and transient Instruments scratch data together.

## Relation to the rest of the code

The UIKit and React Native physical-device benchmark controllers in `xtask::lib` create an `XctraceRecordProcess` instead of a raw `Child`. The owner configures the child, monitors storage, and preserves the final trace only after the caller explicitly commits it.

```text
device benchmark controller
  -> XctraceRecordProcess::spawn
     -> isolated sibling scratch directory
     -> xcrun/xctrace process group
     -> bundle-plus-scratch fuse monitor
  -> trace wait and validation
  -> XctraceRecordProcess::commit
```

## Entry points

- `xtask::xctrace_record::XCTRACE_RECORD_WORKING_SET_LIMIT_BYTES`: the 512 MiB production working-set limit.
- `xtask::xctrace_record::XctraceRecordProcess::spawn(...) -> Result<Self>`: starts a process group with `TMPDIR`, `TMP`, and `TEMP` bound to a unique scratch directory beside the requested trace.
- `XctraceRecordProcess::scratch_path(&self) -> &Path`: exposes the owned scratch path for diagnostics and tests.
- `XctraceRecordProcess::working_set_bytes(&self) -> Result<u64>`: measures the combined retained and transient bytes.
- `XctraceRecordProcess::enforce_working_set_limit(&mut self) -> Result<()>`: synchronously checks the fuse and terminates the process group on overflow or unsafe measurement failure.
- `XctraceRecordProcess::check_fuse(&self) -> Result<()>`: reports a fuse failure detected by the background monitor.
- `XctraceRecordProcess::try_wait_checked(&mut self) -> Result<Option<ExitStatus>>`: probes the child without allowing a storage-fuse failure to be mistaken for a normal exit.
- `XctraceRecordProcess::cleanup_scratch(&mut self) -> Result<()>`: stops monitoring and removes transient scratch after the record child has exited while retaining failure ownership of the final trace.
- `XctraceRecordProcess::commit(&mut self) -> Result<()>`: verifies process completion and storage limits, removes scratch, and transfers the completed final trace to the caller.
- `xtask::xctrace_record::trace_working_set_bytes(...) -> Result<u64>`: computes the overflow-checked combined byte count for a trace and scratch path.

## Logic narrative

Spawn creates a unique sibling scratch directory before starting the child. All three Apple-recognized temporary-directory variables point to that directory, and the child starts in a separate process group so a fuse trip can stop descendants as well as `xcrun`. A monitor measures both paths every 100 ms. Growth beyond 512 MiB, integer overflow, or an unreadable working set fails closed and kills the process group.

The owner is transactional. A normal caller waits for `xctrace`, validates that the bundle settled, and then commits. Any earlier error or ordinary Rust drop terminates the process group, removes scratch, and removes the incomplete final trace. A committed final trace is retained while scratch is still removed.

## Preconditions and postconditions

The output path must have an existing parent, the limit must be positive, and callers must not move unrelated files into the owned scratch directory. Successful commit requires an exited child and a working set within the configured limit. After commit or drop, the scratch directory is absent.

## Edge cases and failure modes

Concurrent file disappearance while Instruments finalizes is treated as zero for a missing path or skipped for a vanished directory entry. Symlinks are counted as links and are never traversed. Measurement failures terminate capture because continuing without a reliable byte bound would recreate the storage-leak risk. Partial final output is deleted unless commit succeeds.

## Concurrency and memory behavior

One bounded monitor thread exists per active trace. It owns cloned paths and two atomics; it does not retain trace contents in memory. Acquire/release ordering ensures the controller observes a tripped fuse before accepting child completion. Drop joins the monitor before deleting its paths.

## Performance notes

Recursive storage accounting runs only in benchmark tooling at 100 ms intervals. It does not affect production apps, renderers, or measured in-app hot paths. The 512 MiB cap matches the Apple comparison controller’s trace working-set policy.

## Feature flags and cfgs

The implementation uses Unix process groups and is intended for Apple-hosted `xtask` execution. It has no feature-flag variants.

## Testing and benchmarks

`tests/xctrace_record_tests.rs` uses shell and sleep stand-ins rather than Instruments. It verifies environment isolation, combined-path accounting, fuse cleanup, child termination on drop, and coverage of every raw `xctrace record` call site in `xtask::lib`.

## Examples

The production entry points are the UIKit and React Native device trace functions in `xtask::lib`; direct use should follow their spawn, wait, settle, and commit sequence.

## Changelog

- 2026-07-21: added isolated scratch ownership, process-group teardown, transactional final-trace retention, and a 512 MiB combined storage fuse for all `xtask` record launches.
