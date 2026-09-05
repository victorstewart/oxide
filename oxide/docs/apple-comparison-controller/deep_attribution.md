# oxide-apple-comparison-controller: deep_attribution

## Intention and purpose

This benchmark-only module acquires one invasive macOS attribution replay at a time. It supports Allocations, VM Tracker, Metal System Trace, and explicit GPU-counter trace configurations without linking any harness code into production Oxide.

## Relation to the rest of the code

- `oxide-benchmark-spec::MacOsAttributionReplaySpec` supplies one frozen replay.
- The existing verified XCUI campaign controller must launch the comparator, admit the AppKit `native.production` audit, discover the exact PID, and start one `MacOsDeepAttributionCollector`.
- The collector attaches native-arm64 xctrace to that PID, uses measured signpost windows, and publishes an atomic hash-bound receipt.
- The existing authoritative analyzer does not consume these rows; their scope remains implementation-only diagnostic attribution.

Call flow:

- campaign scheduler
  - exact comparator PID
  - one replay selector
- `MacOsDeepAttributionCollector::start`
  - isolated scratch directory
  - native-arm64 xctrace process group
  - 512 MiB live fuse
- `finish`
  - graceful interrupt and bounded finalization
  - TOC and schema export in isolated scratch
  - exact-PID measured signpost reduction
  - durable summary, hashes, and completion receipt
- resume
  - finalized trace validation
  - artifact rehash
  - identity/scope validation

## Entry points list

- `MacOsDeepAttributionPaths::new(root: &Path, replay_id: &str, pair_index: u32, side: &str) -> Result<Self>` creates collision-free evidence paths.
- `MacOsDeepAttributionCollector::start(...) -> Result<Self>` starts one exact-PID isolated recorder.
- `MacOsDeepAttributionCollector::wait_for_started(...) -> Result<()>` gates workload start on xctrace readiness.
- `MacOsDeepAttributionCollector::enforce(&mut self) -> Result<()>` checks the live storage fuse.
- `MacOsDeepAttributionCollector::finish(...) -> Result<MacOsDeepAttributionReceipt>` finalizes and publishes evidence.
- `reduce_macos_deep_attribution_exports(...) -> Result<MacOsDeepAttributionSummary>` reduces exported schemas inside exact measured windows.
- `validate_macos_deep_attribution_receipt(...) -> Result<MacOsDeepAttributionReceipt>` validates resumable durable evidence.
- Public summary, schema, metric, export, path, and receipt types expose the diagnostic artifact contract.

## Logic narrative

Only one template or instrument is accepted. xctrace runs in its own process group and receives controller-owned `TMPDIR`, `TMP`, and `TEMP`. A monitor counts the session directory plus scratch every 100 ms and terminates the process group at 512 MiB or when accounting fails. Finalization exports the TOC, signposts, and each exposed data schema into scratch, then removes working XML after parsing. Exact-PID phase signposts define nonoverlapping windows. Data schemas with a process column are filtered to the exact PID; schemas without one retain attached-process/device-window scope explicitly. Numeric raw fields preserve their xctrace mnemonic and exposed unit with count/min/max/sum diagnostics. No output is promoted as a symmetric AppKit/Oxide GPU claim.

## Preconditions and postconditions

The caller provides a nonzero exact PID, nonzero occupied time, a fresh output root, one isolated replay, and a tracing-start notification waiter. Success leaves a finalized trace, TOC, signposts, summary, and completion receipt; scratch is removed.

The collector is deliberately not exposed through a standalone campaign command yet. Wiring it to the verified XCUI controller, bundled full-attribution execution plan, comparator-audit admission, durable 13-scenario completion receipts, and automatic app dismissal remains required before live acquisition is enabled. An arbitrary external driver is not an accepted substitute.

## Edge cases and failure modes

Reused paths, merged selectors, zero PID/time, recorder exit before readiness, timeouts, fuse breach, failed storage accounting, incomplete trace bundles, missing signposts, overlapping phases, schemas without timestamps, duplicate schemas, export failures, hash drift, and receipt identity drift fail closed. Drop terminates the entire recorder process group and removes unpublished evidence.

## Concurrency and memory behavior

One monitor thread owns no app state and checks storage only. Reduction owns bounded XML strings after recorder finalization. No work enters the measured comparator process.

## Performance notes

These are intentionally invasive diagnostic replays. They are never combined without a separately accepted calibration and never replace the low-overhead headline collectors.

## Feature flags and cfgs

None. Live acquisition requires macOS xctrace; pure reduction tests are platform-independent Rust tests.

## Testing and benchmarks

`tests/deep_attribution_tests.rs` covers exact-PID filtering, measured-window isolation, raw units/metrics, and missing/overlapping phase rejection. Live schema proof remains verification-pending until the headed macOS comparator campaign can run.

## Examples

Construct paths for one plan replay and pair/side, start the collector after exact PID discovery, wait for its notification, execute the unchanged scenario sequence, and call `finish` after durable comparator completion.

## Changelog

- 2026-07-21: added isolated full-attribution acquisition, generic schema reduction, storage fuse, and atomic resume evidence.
