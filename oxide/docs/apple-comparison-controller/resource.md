# `oxide-apple-comparison-controller` `src/resource.rs`

## Intention and purpose

This module collects comparable macOS process CPU, runnable time, wakeup, page-in, memory, storage, logical-write, instruction, cycle, and exposed energy counters without linking benchmark logic into either measured application.

## Relation to the rest of the code

`src/lib.rs` starts `MacOsResourceCollector` immediately after exact executable PID discovery for every measured session. The collector calls macOS `proc_pid_rusage` with `RUSAGE_INFO_V4`, returns a typed artifact after durable app completion, and `src/lib.rs` atomically persists and hashes that artifact before writing the controller receipt. Correctness sessions use `MacOsResourceArtifact::not_applicable`.

## Entry points list

The collector is crate-private. Its controller-facing entries are:

- `MacOsResourceCollector::start(pid: u32, pass_id: &str) -> Result<Self>` validates an immediate first snapshot and starts the pass-specific fixed-cadence sampler.
- `MacOsResourceCollector::finish(self, identity: &MacOsResourceIdentity<'_>, durable_complete_timestamp: u64) -> Result<MacOsResourceArtifact>` requests a terminal sample, joins the sampler, and validates the completed artifact.
- `MacOsResourceArtifact::not_applicable(identity: &MacOsResourceIdentity<'_>, durable_complete_timestamp: u64) -> Self` creates the explicit untimed-correctness record.
- `validate_resource_artifact(artifact: &MacOsResourceArtifact, identity: &MacOsResourceIdentity<'_>) -> Result<()>` enforces identity, availability, coverage, and monotonicity.
- `reduce_macos_resource_artifact(bytes: &[u8], window_start_ticks: Option<u64>, window_end_ticks: Option<u64>) -> Result<MacOsResourceSummary>` is the public offline reducer for a complete schema-v4 artifact.

## Logic narrative

The controller takes the first sample synchronously so launch fails closed if V4 resource accounting is unavailable. A dedicated thread then samples against monotonic pass-specific deadlines. Presentation, launch, CPU, scheduler, common-GPU, and full-attribution passes use 50 ms. Idle, endurance, energy, memory, and physical-footprint passes use the contract's low-frequency one-second cadence to avoid turning a soak into a collector-overhead benchmark. Each snapshot must retain the initial process UUID and process-start absolute time, preventing PID reuse from crossing the stream. A finish command causes one terminal snapshot after durable completion; an abort command ends without publishing. The owner always joins the thread, including through `Drop` on an error path.

The artifact records cumulative user/system CPU and runnable nanoseconds, package-idle and interrupt wakeups, page-ins, disk bytes, logical writes, instructions, cycles, billed/serviced system time, and raw billed/serviced energy fields. Current wired, resident, and physical footprint plus lifetime and interval maximum footprint are retained from the same V4 snapshot. Schema v4 also records the nonzero `mach_timebase_info` numerator and denominator needed to convert every persisted continuous-clock sample without making a machine-specific assumption. Validation requires strictly increasing `mach_continuous_time` samples and nondecreasing cumulative counters. Wired, resident, current physical-footprint, and interval-maximum values may legitimately fall and are not treated as cumulative. Schema v4 requires the complete field set for measured sessions; legacy schemas remain resumable only for untimed correctness sessions whose sample list is necessarily empty.

The offline reducer can select an inclusive continuous-clock window, converts it using the artifact's recorded timebase, and reports CPU milliseconds per wall second, runnable time, wakeups per wall second, page-ins, disk and logical-write deltas, instruction/cycle and billed/serviced deltas, wired/resident/physical-footprint start/end/peak, and a Theil-Sen retained-footprint slope in bytes per minute. Claim-bearing analysis supplies the measured-phase start/end ticks from validated telemetry; reducing the whole process is only a diagnostic default. The reducer rejects incomplete streams, legacy measured schemas, reversed or undersampled windows, and counter rollback instead of publishing a partial summary.

## Preconditions and postconditions

The PID is the exact executable PID discovered by the controller and fits the signed Darwin PID range. The FFI buffer has the SDK-defined 296-byte `rusage_info_v4` layout. A successful measured artifact has at least two samples, starts after controller launch time, ends no earlier than durable completion, and binds the run, plan, generation, side, pack, PID, and executable SHA-256.

## Edge cases and failure modes

Unavailable libproc accounting, process exit, PID reuse, sampler panic, channel failure, missing terminal coverage, malformed hashes, identity drift, legacy measured schemas, or cumulative CPU/wakeup/page-in/I/O/instruction/cycle/energy/logical-write/runnable-time rollback fail the session. A process may report zero for counters unavailable to that OS/device; the raw zero remains explicit.

## Concurrency and memory behavior

Exactly one bounded sampler thread exists per measured process. The sample vector reserves 512 rows and grows only in the controller process, never in the measured application. Shutdown is synchronous and deterministic.

## Performance notes

The artifact records the exact 50 ms or one-second cadence selected by its pass, and validation rejects a mismatched declaration. The resource fields are read from one `proc_pid_rusage` snapshot, so adding a field does not add another syscall, thread, or measured-app operation. The collector is comparison-harness code, but each complete sampling profile still requires instrumentation-disabled calibration before authoritative claims.

## Feature flags and cfgs

The libproc FFI and V4 sampling implementation compile only on macOS. The non-macOS path fails explicitly.

## Testing and benchmarks

Module tests reject PID mismatch, measured schema-v1 input, and cumulative cycle/runnable-time rollback; they accept the explicit correctness not-applicable shape and its empty legacy schema-v1 form. A deterministic reducer test freezes timebase conversion, CPU/wakeup rates, interval peak selection, and robust footprint slope. The integration source contract freezes controller ownership, cadence, V4 flavor, terminal sampling, and resource hash binding.

## Examples

The collector is driven only by `run_macos_campaign`; callers do not start it directly.

## Changelog

- 2026-07-19: introduced fixed-cadence exact-process V4 resource collection.
- 2026-07-21: made resource cadence pass-specific, using one-second sampling for soak, energy, memory, and physical-footprint acquisitions.
- 2026-07-21: persisted the Darwin timebase in measured schema-v4 resource artifacts so reducers can convert continuous-clock windows exactly.
- 2026-07-21: added the schema-v4 offline process-resource reducer and robust retained-footprint slope.
- 2026-07-19: retained wakeup, page-in, wired-memory, logical-write, and runnable-time fields already returned by the same V4 snapshot and required schema v2 for measured sessions.
