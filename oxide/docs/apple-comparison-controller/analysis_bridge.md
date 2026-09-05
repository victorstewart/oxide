# macOS generic analyzer bridge

## Intention and purpose

`analysis_bridge` converts one completed, authoritative-eligible macOS campaign pass/pack into the framework-neutral comparison analyzer's canonical directory contract. It is benchmark-only code in `oxide-apple-comparison-controller`; no production Oxide or AppKit binary links it.

## Relation to the rest of the code

The bridge reads controller-owned `campaign.plan.json`, `campaign.complete.json`, atomic pair checkpoints, schema-4 exact-process resource streams, and the validated artifact family selected by the analyzer plan. That family may be presentation correlation, a whole-session resource summary, canonical launch evidence, phase-bounded common-GPU evidence, or direct-meter energy evidence. A separately preregistered `ComparisonPlan` selects exactly one pass and one pack. Successful conversion atomically publishes:

- `plan.json`: the exact generic analyzer plan bytes.
- `sessions.jsonl`: typed `ComparisonSession` records.
- `raw/<generation>.jsonl`: typed `RawObservationRow` records.
- `manifest.json`: hashes for the source campaign, analyzer plan, sessions, and every raw artifact.

The output can be passed unchanged to `compare-ui analyze --input`.

## Entry points

- `materialize_macos_analyzer_bundle(campaign_root, analyzer_plan_path, output_root)` validates and atomically publishes one bundle.
- `MacOsAnalyzerBundleManifest` records every published artifact identity and session count.
- `MACOS_INPUT_TO_PRESENT_METRIC_SOURCE` maps a required nanosecond metric to `candidate_input_to_frame_lifetime_end_ns`.
- `MACOS_VISUAL_TO_PRESENT_METRIC_SOURCE` maps a required nanosecond metric to `candidate_visual_to_frame_lifetime_end_ns`.
- `MACOS_RESOURCE_METRIC_SOURCE_PREFIX` selects validated `RUSAGE_INFO_V4` CPU, runnable time, wakeup, page-in, I/O, resident/physical-footprint, and retained-slope summary fields.
- `MACOS_LAUNCH_METRIC_SOURCE_PREFIX` selects validated exact-PID launch and first-interactive milestone deltas.
- `MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX` selects phase-bounded `TASK_POWER_INFO_V2` GPU time and remains descriptive because the reducer records compositor-ownership asymmetry.
- `MACOS_ENERGY_METRIC_SOURCE_PREFIX` selects phase-bounded direct-meter joules or watts, including explicitly named baseline-adjusted fields.

Source fields are appended after the prefix. Resource fields are `wall-time-ns`, `user-cpu-ns`, `system-cpu-ns`, `process-cpu-ms-per-wall-s`, `runnable-time-ns`, `wakeups`, `wakeups-per-wall-s`, `pageins`, `disk-read-bytes`, `disk-written-bytes`, `logical-writes`, `instructions`, `cycles`, the `wired-*`, `resident-*`, and `physical-footprint-*` byte fields, and `retained-slope-bytes-per-min`. Launch fields cover request-to-complete-generation, request-to-attributed-present, input-request-to-response-generation, and input-request-to-response-attributed-present in nanoseconds. Common GPU exposes only `phase-gpu-time-ns`. Direct energy exposes phase joules, average watts, and their explicitly baseline-adjusted forms.

## Logic narrative

The bridge requires materialized campaign schema 2 and completed report schema 4 in `Full` scope. Acquisition, correctness, measured-acquisition eligibility, and authoritative eligibility must all be true. Before converting rows, it rereads the report-bound `acquisition.validity.json`, verifies its hash and run/plan/build identities, binds every opportunity count and calibration status to the actual primary-session correlation artifact, and binds measured-input scope to each Primary complete envelope plus any session-owned raw-receipt manifest. The report's complete ordered session list must equal the materialized campaign, and the generic analyzer plan must bind the same source-plan SHA-256 and content-derived seed. Its implementation roles are fixed to `native.production` as reference A and `oxide.production` as contender B, with an accepted native comparator.

Analyzer v1 identifies a session by pass, pair, and implementation but has no pack field. To prevent two packs with the same pair index from becoming ambiguous, each output bundle must select exactly one scenario pack. Every controller chunk and comparison cell must select that same pass and pack. Required metrics must use an explicit supported source and its exact unit; unsupported sources, field names, units, and comparison-ineligible common-GPU claim cells fail before publication. A whole-session resource summary is admitted only for a one-scenario pack, so CPU, idle, endurance, or memory totals cannot be cloned across packed scenarios without phase boundaries.

For each pair, the bridge validates analyzer order against actual native/Oxide side order, requires contiguous pair indices, reopens the durable pair checkpoint, verifies its stored session results and build identity, and checks the selected checkpoint hash chain. It always reopens and validates the deterministic campaign-root resource stream. For every selected non-launch session it also independently reopens the complete envelope, `telemetry.bin`, `telemetry.coverage.json`, and surface receipt after campaign validation; it recomputes every hash, validates exact campaign-relative paths, reconciles telemetry coverage against the binary ring, validates the surface identity/shape, and carries those hashes into the analyzer session's pass-artifact identity. Presentation rows additionally require complete exact-PID correlation. Resource rows are regenerated through `reduce_macos_resource_artifact`. Launch rows bind the launch evidence to the exact resource PID/timebase and its launch correlation. Common-GPU rows re-run `reduce_macos_common_gpu` from hash-bound samples and signposts. Energy rows re-run `reduce_macos_energy` from hash-bound raw samples, comparator telemetry, and the direct-meter calibration embedded in the summary. No raw `billed_energy` counter is relabeled as direct energy, and missing optional artifacts never become zeros or proxies.

All validation and conversion happens before the destination exists. Publication writes to a destination-specific staging directory and renames it only after every file and manifest is complete. Failure removes staging and never publishes a partial analyzer bundle.

## Preconditions and postconditions

The campaign and analyzer plan must be immutable, content-addressed, complete, and authoritative-eligible. The output path must not exist. Success produces normalized relative raw paths, decimal-string u64 identities, analyzer-spelled `ab`/`ba` order, numeric observations within JavaScript's exact integer range, and a manifest binding every output.

## Edge cases and failure modes

Incomplete or diagnostic-only campaigns, pending comparator status, source-plan or seed drift, multiple packs, ambiguous or missing pairs, executable drift, noncanonical recorded paths, changed telemetry/coverage/surface evidence, telemetry coverage disagreement, invalid surface receipts, hash mismatch, invalid resource timebase, PID mismatch, uncorrelated visuals, unsupported metric sources/fields/units, multi-scenario whole-session resource assignment, comparison-ineligible GPU claims, absent direct-meter evidence, and preexisting output or staging paths fail closed.

## Performance notes

Conversion runs after acquisition and outside every measured process. It performs bounded artifact reads, JSON decoding, row materialization, hashing, and one atomic directory rename. It does not affect production performance.

## Testing

`tests/analysis_bridge_tests.rs` constructs complete authoritative controller evidence, proves presentation and validated resource-summary row publication, proves independently reopened telemetry/coverage/surface tampering prevents publication, and proves ineligible campaigns and unsupported required metrics leave no published directory.

## Changelog

- 2026-07-21: independently reopened, rehashed, and revalidated telemetry, telemetry coverage, and surface receipts at analyzer publication.
- 2026-07-21: added validated CPU/wakeup/memory, launch, descriptive common-GPU, and direct-energy source families plus the one-scenario whole-resource and no-proxy admission rules.
- 2026-07-21: introduced the fail-closed one-pass/one-pack macOS campaign-to-generic-analyzer bridge.
